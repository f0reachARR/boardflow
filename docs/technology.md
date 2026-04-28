# BoardFlow 技術スタック案

## 1. 前提

`docs/spec.md` の MVP を、以下の性質を持つサービスとして実装する。

- GitHub Actions 上の Docker Action が KiCad 成果物を生成する
- SaaS は成果物の受信、保存、表示、差分判定、GitHub Issue 連携を担当する
- API 契約は OpenAPI を中心に管理する
- サーバーは Go、フロントエンドは Next.js を基本方針とする
- DB は PostgreSQL を主データストアにする
- Redis はキャッシュではなく、主にジョブ・レート制御・短期状態管理に使う

この方針は合理的である。特に、Action 側と SaaS 側の境界がはっきりしているため、OpenAPI で API 契約を固定し、Go で型安全な API サーバーと CLI/Action 補助ツールを作る構成と相性がよい。

## 2. 推奨スタック概要

| 領域 | 採用候補 | 理由 |
|---|---|---|
| API 仕様 | OpenAPI 3.0.3 | Action / SaaS / 将来の公開 API の契約を一元化でき、Go の生成ツールと合わせやすい |
| API サーバー | Go | 単一バイナリ、並行処理、GitHub API ジョブ、アップロード処理に向く |
| Go HTTP ルータ | chi | `net/http` 互換で薄く、OpenAPI 生成コードと合わせやすい |
| OpenAPI Go 生成 | oapi-codegen | Go の型、strict server、chi server、client 生成を利用できる |
| DB | PostgreSQL | Repository / BoardProject / BoardRun / Artifact の正規データに向く |
| Go DB アクセス | sqlc + pgx | SQL を明示しつつ型安全な Go コードを生成できる |
| Migration | goose または Atlas | MVP では goose が軽量。将来スキーマ管理を強めるなら Atlas |
| 非同期ジョブ | PostgreSQL backed queue から開始 | GitHub Issue 連携は永続性が重要。MVP は Redis 専用より DB キューが安全 |
| Redis | rate limit / debounce / lock / short cache | GitHub API のレート制御や重複抑制に使う |
| Artifact 保存 | S3 互換オブジェクトストレージ | PDF/SVG/HTML/ZIP を DB に入れず、署名付き URL でアップロード/配信する |
| Frontend | Next.js App Router + TypeScript | Server Components で API 取得し、必要箇所だけ Client Component にできる |
| UI | Chakra UI + lucide-react | アクセシブルな React コンポーネントを使い、SaaS 管理画面を短期間で整えやすい |
| 認証 | GitHub OAuth / GitHub App installation ベース | GitHub リポジトリ連携サービスなので GitHub 中心が自然 |
| 本番ホスティング | VPS | API、worker、Next.js、PostgreSQL/Redis 接続を自前運用する前提にする |
| Observability | OpenTelemetry + structured logging | Action/API/GitHub job の追跡に必要 |
| Local dev | Docker Compose | Postgres、Redis、MinIO をローカルに揃える |

## 3. API と OpenAPI

OpenAPI は `api/openapi.yaml` を canonical source とする。MVP では OpenAPI 3.0.3 を推奨する。3.1 は JSON Schema との整合性が高い一方、Go のコード生成や周辺ツールで 3.0 系の方が安定している場面があるため、生成ツール互換を優先する。

推奨構成:

- `POST /api/v1/runs/plan`
- `POST /api/v1/board-runs`
- `POST /api/v1/board-runs/{board_run_id}/complete`
- `POST /api/v1/board-runs/{board_run_id}/fail`
- Web UI 用の read API
- Artifact ダウンロード/プレビュー用 API
- GitHub App webhook API

GitHub App webhook は MVP に含める。installation 情報の同期、repository 権限確認、将来の uninstall / suspend 対応の土台になるため、Action token 発行を簡略化する場合でも webhook endpoint と署名検証は最初から持つ。

Go サーバー側は `oapi-codegen` で以下を生成する。

- API models
- chi server
- strict server interface
- 必要に応じて Action 用 Go client

OpenAPI から TypeScript 型を生成する場合は、`openapi-typescript` を候補にする。Next.js から API を呼ぶ箇所では、生成型を使って UI と API のずれを早めに検出する。

## 4. Backend

Go サーバーは、HTTP API と worker を同一リポジトリ内の別プロセスとして持つ構成を推奨する。

```text
cmd/api
cmd/worker
internal/api
internal/domain
internal/db
internal/github
internal/artifact
internal/jobs
```

HTTP 層は chi + oapi-codegen に寄せ、業務ロジックは `internal/domain` に置く。GitHub API、Artifact storage、DB はインターフェースで薄く分離する。

DB アクセスは sqlc + pgx を推奨する。ORM より SQL が見えた方が、`repository_id + project_path` の unique 制約、latest run 更新、ジョブの排他取得などを明示しやすい。

## 5. Database

主データは PostgreSQL に保存する。

採用理由:

- `repositories`、`board_projects`、`board_runs`、`artifacts` の関連が明確
- unique 制約と transaction が重要
- GitHub job の冪等性を DB 制約で守りやすい
- JSONB で manifest や file hashes を柔軟に保存できる

`board_project_snapshots.file_hashes_json` は MVP でも保存してよい。ストレージ量は増えるが、将来の差分表示やデバッグに効く。

ID は外部露出用に `bp_...`、`br_...` のような prefix ID を使う。内部 DB 主キーは UUID または ULID を推奨する。時系列ソートやログ追跡を考えると ULID が扱いやすい。

## 6. Queue / Worker

GitHub API 操作はすべて非同期 worker で処理する。

MVP では PostgreSQL backed queue を推奨する。理由は、Issue 作成やコメント更新は失われてはいけない処理であり、DB transaction と unique 制約で冪等性を守りやすいため。

Redis は以下に限定して使う。

- installation / repository 単位の短期 rate limit 状態
- dashboard comment update の debounce
- worker 間の軽量 lock
- 一時的な idempotency cache

将来的にジョブ量が大きくなったら、River、Temporal、Cloud Tasks、SQS などへの移行を検討する。

## 7. Artifact Storage

成果物は S3 互換オブジェクトストレージに保存する。Artifact は private 前提とし、DB には metadata と storage key のみを保存する。

候補:

- 本番: S3 互換ストレージ
- ローカル: MinIO

アップロードは API サーバーが署名付き URL を発行し、Action が直接 PUT する方式を推奨する。API サーバーを大きな ZIP/PDF の中継点にしないことで、負荷とタイムアウトを避ける。

iBOM HTML などの成果物表示は、CORS とセキュリティ境界を明確にするため、通常のアプリとは別ドメインで配信する。private artifact なので、閲覧時は短命の署名付き URL または artifact proxy が発行する短命アクセス URL を使う。

想定ドメイン:

- App: `https://boardflow.example.com`
- API: `https://api.boardflow.example.com`
- Artifact: `https://artifacts.boardflow.example.com`

Artifact ドメインでは、少なくとも以下を設定する。

- artifact 専用 domain/subdomain
- `Content-Security-Policy`
- 必要な origin のみ許可する `Access-Control-Allow-Origin`
- `X-Content-Type-Options: nosniff`
- iframe sandbox

## 8. Frontend

Next.js App Router + TypeScript を採用する。

Next.js は Server Components をデフォルトにできるため、Repository ページ、BoardProject ページ、Runs 一覧のような読み取り中心画面に向いている。PCB preview、タブ操作、iBOM iframe、フィルタなど、ブラウザ状態が必要な箇所だけ Client Component にする。

推奨構成:

```text
app/
  repositories/[repositoryId]/page.tsx
  repositories/[repositoryId]/boards/[boardProjectId]/page.tsx
  repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx
components/
lib/api/
```

UI は Chakra UI + lucide-react を採用する。Chakra UI は Next.js App Router では root に client component の Provider を置く構成にする。業務系 SaaS なので、装飾よりも一覧性、ステータス視認性、成果物への導線を優先する。

```text
app/provider.tsx
  ChakraProvider
app/layout.tsx
  Provider で children を包む
```

## 9. Authentication / Authorization

認証は GitHub 中心にする。

MVP の推奨:

- ユーザーのログイン: GitHub OAuth
- リポジトリ連携: GitHub App installation
- Action から SaaS への認証: BoardCI API token

Action token は repository または installation に紐づける。漏洩時に失効できるよう、token hash のみ DB に保存する。

MVP では GitHub と同様の閲覧権限にする。GitHub App installation にアクセスでき、対象 repository への権限を持つ GitHub ユーザーだけが、その repository と BoardProject を閲覧できる。

将来 GitHub 以外の Git provider を見る場合は権限モデルを抽象化するが、MVP では GitHub に寄せてよい。

## 10. Deployment

本番ホスティングは VPS 前提とする。

VPS 上で動かす主要プロセス:

- Go API server
- Go worker
- Next.js server
- reverse proxy
- PostgreSQL
- Redis

S3 互換ストレージは VPS 外部に用意する前提にする。ローカル開発では MinIO を使う。

reverse proxy では App / API / Artifact のドメインを分ける。

```text
boardflow.example.com           -> Next.js
api.boardflow.example.com       -> Go API
artifacts.boardflow.example.com -> artifact proxy or object storage origin
```

Artifact は private 前提のため、object storage を直接 public bucket として公開しない。署名付き URL、短命 cookie、または artifact proxy のいずれかで認可済みユーザーだけに返す。

## 11. Docker Action

Docker Action は Go または Python の薄い CLI をエントリポイントにし、KiCad / iBOM / 補助スクリプトをコンテナに含める。

推奨は以下。

- Action orchestration: Go CLI
- KiCad / iBOM 実行補助: 必要に応じて Python
- API client: OpenAPI から生成した Go client
- Container base: KiCad 9.0 系が安定して入る Ubuntu/Debian 系

ハッシュ計算、`.boardci.yml` 読み込み、manifest 作成、アップロード制御は Go で実装すると、SaaS API client と型を共有しやすい。

## 12. Observability

最初から structured logging と trace id を入れる。

最低限必要な相関 ID:

- `github_repository_id`
- `project_path`
- `board_project_id`
- `board_run_id`
- `github_run_id`
- `github_run_attempt`
- `commit_sha`

OpenTelemetry は HTTP API、DB、GitHub API worker、object storage 操作に入れる。MVP ではログだけでも始められるが、Action と SaaS の境界で失敗するサービスなので、早めに trace を入れる価値が高い。

## 13. Testing

Backend:

- domain logic unit test
- OpenAPI request/response contract test
- DB repository test with PostgreSQL
- GitHub webhook signature verification test
- worker idempotency test

Frontend:

- component test
- Playwright による主要画面の smoke test

Action:

- `.boardci.yml` discovery test
- tree hash test
- exclude pattern test
- manifest generation test
- API client mock test

KiCad 実行を含む E2E は重いので、MVP では nightly または手動に回してよい。

## 14. 合理的でない可能性がある選択

### Redis を主キューにする

GitHub Issue 作成やコメント更新は失いたくない処理なので、MVP で Redis のみを永続キューにするのは避けたい。PostgreSQL にジョブを保存し、Redis は補助にする方が安全。

### Next.js API Routes に SaaS API を寄せる

OpenAPI 契約、Action からのアップロード、GitHub webhook、worker との共有を考えると、API は Go サーバーに寄せる方がよい。Next.js は Web UI に集中させる。

### Artifact を DB に保存する

PDF、SVG、HTML、ZIP はサイズが大きくなりやすい。DB には metadata と storage key のみを保存し、本体は object storage に置く。

### SaaS 側で KiCad を実行する

仕様の基本方針どおり避ける。依存関係、フォント、ライブラリ、CPU 時間、セキュリティ境界が重くなる。

## 15. 決定済み前提

今回の前提として、以下を決定済みにする。

1. 本番ホスティングは VPS

2. Artifact 保存は S3 互換ストレージ

3. Web UI の閲覧権限は GitHub と同様

4. Artifact は private 前提

5. GitHub App webhook は MVP に含める

6. iBOM などの成果物ホスティングは CORS 対応とセキュリティ境界のため別ドメイン

## 16. MVP 推奨結論

MVP は以下で進める。

```text
OpenAPI 3.0.3
Go + chi + oapi-codegen
PostgreSQL + sqlc + pgx
PostgreSQL backed worker queue
Redis for rate/debounce/lock
S3-compatible object storage + presigned upload/download
Next.js App Router + TypeScript
Chakra UI + lucide-react
GitHub OAuth + GitHub App
GitHub App webhook
VPS deployment
separate artifact domain
Docker Compose for local dev
OpenTelemetry + structured logs
```

この構成なら、仕様の中核である `BoardProject = repository_id + project_path`、Action による成果物生成、SaaS による状態管理、GitHub Issue 連携を無理なく実装できる。
