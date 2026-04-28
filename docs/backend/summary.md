# Backend 技術方針サマリ

BoardFlow の backend は、GitHub Actions から受け取った成果物と状態を正しく管理し、Web UI と GitHub 連携に安全に供給するための中核である。MVP では、拡張性より先に、契約の明確さ、冪等性、運用時の追跡しやすさを優先する。

## 1. 役割

backend の責務は以下。

- OpenAPI ベースの API 提供
- BoardProject / BoardRun / Artifact の永続化
- 差分判定 API の提供
- GitHub App webhook 受信と連携状態の同期
- GitHub Issue 作成、コメント更新などの非同期処理
- Artifact への認可付きアクセス提供

KiCad 実行そのものは GitHub Actions 上の Docker Action が担当する。

## 2. 採用スタック

| 領域 | 採用方針 | 理由 |
|---|---|---|
| API 仕様 | OpenAPI 3.0.3 | Action / SaaS / 将来の公開 API の契約を一元化しやすい |
| API サーバー | Go | 単一バイナリ、並行処理、アップロード制御、GitHub API 連携に向く |
| HTTP ルータ | chi | `net/http` 互換で薄く、生成コードと合わせやすい |
| OpenAPI 生成 | oapi-codegen | models、strict server、chi server、client を生成できる |
| DB | PostgreSQL | 制約、transaction、JSONB を活かしやすい |
| DB access | sqlc + pgx | SQL を明示しつつ型安全なコードを得られる |
| Migration | goose または Atlas | MVP は軽さ優先で goose、有力候補として Atlas |
| Queue | PostgreSQL backed queue | 冪等性と永続性を優先できる |
| Redis | rate limit / debounce / lock / short-lived state | 補助用途に限定できる |
| Artifact storage | S3-compatible object storage | 大きな成果物を DB から分離できる |
| Observability | OpenTelemetry + structured logging | Action から worker まで追跡しやすい |

## 3. API 契約

OpenAPI を `api/openapi.yaml` のような canonical source として管理する。

MVP で重要な API 群:

- `POST /api/v1/runs/plan`
- `POST /api/v1/board-runs`
- `POST /api/v1/board-runs/{board_run_id}/artifact-bundles/import`
- `POST /api/v1/board-runs/{board_run_id}/complete`
- `POST /api/v1/board-runs/{board_run_id}/fail`
- Web UI 向け read API
- Artifact ダウンロード / プレビュー用 API
- GitHub App webhook API

特に Action と SaaS の境界は、実装より先に契約を固定した方が安全である。

## 4. サービス構成

同一リポジトリ内で、API と worker を別プロセスとして持つ構成を基本にする。

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

設計上のポイント:

- HTTP 層は薄く保つ
- 業務ロジックは `internal/domain` に寄せる
- GitHub API、artifact storage、DB は明確な境界で分ける
- worker からも再利用できるユースケースにしておく

## 5. データモデル

主データは PostgreSQL に保存する。

重要な理由:

- `repositories`、`board_projects`、`board_runs`、`artifacts` の関係が明確
- unique 制約と transaction が重要
- GitHub job の冪等性を DB 制約で守りやすい
- manifest や file hashes を JSONB で柔軟に扱える

MVP で特に重要な識別:

- BoardProject の同一性は `github_repository_id + project_path`
- 外部公開用 ID は `bp_...`、`br_...` などの prefix を持たせる
- 内部主キーは UUID または ULID を採用する

`board_project_snapshots.file_hashes_json` のような差分追跡用データは、MVP でも持つ価値が高い。

## 6. Queue / Worker

GitHub API 操作と artifact import / 解析は非同期 worker で処理する。

MVP で PostgreSQL backed queue を推す理由:

- Issue 作成やコメント更新を失いたくない
- DB transaction と一緒にジョブ生成しやすい
- unique 制約で冪等性を設計しやすい
- 再実行や失敗追跡が行いやすい

Redis は以下の補助に限定する。

- installation / repository 単位の短期 rate limit 状態
- dashboard comment update の debounce
- worker 間の軽量 lock
- 一時的な idempotency cache

artifact import job は、少なくとも以下の段階を持つ。

- staging object の存在確認
- zip / manifest の検証
- artifact 展開と final bucket への保存
- DRC / ERC / manifest の解析
- DB への run summary / review data / snapshot 保存
- BoardRun の completed または failed への遷移

## 7. Artifact Storage

成果物の正本は S3 互換オブジェクトストレージに保存し、DB には metadata と storage key のみを保存する。

推奨構成:

- 本番: S3-compatible object storage
- ローカル: MinIO
- staging bucket: Action が zip bundle を一時配置するために使う
- final bucket: backend が検証済み artifact を保存する

MVP では、成果物は individual file upload ではなく staging zip import を基本にする。

- API が発行した presigned URL で Action が staging bucket に zip を置く
- Action は import API を呼び、backend は artifact import job を queue に積む

backend は zip を受け取っただけでは完了扱いにせず、以下を行ってから確定する。

- manifest の schema 検証
- required artifact の有無確認
- path traversal や危険な entry 名の拒否
- sha256 / size / content type の検証
- 展開後 artifact の final bucket への保存
- artifact metadata / run summary / snapshot の DB 保存
- DRC / ERC のレビュー用明細の DB 保存

private artifact 前提なので、配信時は以下を前提にする。

- artifact 専用 domain / subdomain
- 短命の署名付き URL または proxy 経由
- `Content-Security-Policy`
- 制限付き `Access-Control-Allow-Origin`
- `X-Content-Type-Options: nosniff`
- iframe sandbox

## 8. 認証と認可

MVP では GitHub 中心の構成にする。

- ユーザーログイン: GitHub OAuth
- リポジトリ連携: GitHub App installation
- Action から SaaS への認証: BoardCI API token

考慮点:

- token は hash のみ DB 保存
- GitHub App webhook は MVP から含める
- installation 情報同期と権限確認を backend の責務にする
- Web UI の閲覧可否は GitHub 権限と揃える

## 9. デプロイ

本番ホスティングは VPS 前提とする。

主な実行要素:

- Go API server
- Go worker
- Next.js server
- reverse proxy
- PostgreSQL
- Redis

ドメイン分離の前提:

```text
boardflow.example.com           -> Next.js
api.boardflow.example.com       -> Go API
artifacts.boardflow.example.com -> artifact proxy or object storage origin
```

Artifact は public bucket として直接公開しない。

## 10. Docker Action との境界

Docker Action 側は以下を担当する。

- `.boardci.yml` 探索
- project_path 決定
- tree hash / manifest 生成
- KiCad / iBOM 実行
- plan API 呼び出し
- zip bundle 作成
- staging upload
- import 要求
- run fail 通知

backend 側は、Action の入出力契約と冪等性を守る責務を持つ。

## 11. 監視と追跡

最初から structured logging と trace id を入れる。

最低限必要な相関 ID:

- `github_repository_id`
- `project_path`
- `board_project_id`
- `board_run_id`
- `github_run_id`
- `github_run_attempt`
- `commit_sha`

OpenTelemetry は HTTP API、DB、GitHub API worker、object storage 操作に入れる。

## 12. テスト方針

MVP の backend test は以下を基準にする。

- domain logic unit test
- OpenAPI request / response contract test
- DB repository test with PostgreSQL
- GitHub webhook signature verification test
- worker idempotency test
- artifact import worker test
- DRC / ERC parser test

Action を含む重い E2E は nightly または手動でもよいが、OpenAPI 契約テストは早めに固めたい。

## 13. 避けたい選択

### Redis を主キューにする

重要ジョブの永続性と冪等性が弱くなりやすい。

### Next.js API Routes に SaaS API を寄せる

Action 連携、OpenAPI 管理、worker 共有を考えると責務が濁る。

### Artifact を DB に保存する

サイズ効率も配信設計も悪くなりやすい。

### SaaS 側で KiCad を実行する

依存関係、実行コスト、セキュリティ境界が重い。

## 14. 今後の深掘り候補

- OpenAPI ファイルの分割方針
- `board_runs` / `artifacts` / `snapshots` の詳細 schema
- queue 実装を自前にするか既存ライブラリを使うか
- zip intake / staging import の失敗回復設計
- GitHub App と BoardCI token のライフサイクル
- artifact proxy を置くか、署名付き URL のみで始めるか
