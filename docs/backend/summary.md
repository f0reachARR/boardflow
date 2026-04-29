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
- `POST /api/v1/board-runs/{board_run_id}/fail`
- `GET /api/v1/board-runs/{board_run_id}/viewer-sources`
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
- BoardFlow API token は repository 単位で `installation_id + github_repository_id` に紐づける

`board_project_snapshots.file_hashes_json` のような差分追跡用データは、MVP でも持つ価値が高い。
`board_projects.latest_successful_run_id` のような名前はDRC/ERC成功と混同しやすいため、artifact import成功を表す `latest_completed_run_id` に寄せる。
Web UI の通常一覧には初回 completed 前の BoardProject も状態付きで表示し、検出済み、処理中、失敗、timeout、completed を追えるようにする。
close済みIssueから新Issueへ切り替える場合に備え、過去Issueは `board_project_issue_history` 相当の履歴テーブルに残す。
`recreate_issue_on_update` はMVPではデフォルト `true` とし、close済みIssueに対して変更が入った場合は新Issueを作る運用を基本にする。

BoardRun の状態は成果物生成、upload、import の状態を表し、DRC/ERC の成功失敗とは分ける。

```text
created
uploading
importing
completed
failed
timed_out
```

`completed` は artifact import が成立したことを表し、DRC/ERC の成功を意味しない。
DRC/ERC が failed でも、manifest とチェック結果または skipped 状態を保存できた場合、BoardRun は `completed` として扱う。
個別 artifact の `missing` / `failed` / `skipped` は警告として保存し、それだけでは BoardRun や GitHub Actions job を失敗にしない。
`fail-on-drc` / `fail-on-erc` によるGitHub Actions job失敗はCI gateであり、BoardRunを `failed` にする理由にはしない。
BoardRun 作成から12時間以内に `completed` または `failed` へ到達しない場合、worker が `timed_out` に遷移させる。
GitHub Actions の cancel、runner 停止、fail API 未送信の異常終了も MVP では `timed_out` に集約する。
`board_project_id + github_run_id + github_run_attempt` は冪等キーとして扱い、同一 attempt の再送は terminal 状態を含めて既存 BoardRun を返す。

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
- BoardRun の `completed` または `failed` への遷移
- `latest_tree_hash` と `latest_completed_run_id` の更新
- 初回 completed run で Issue 未作成の場合の Issue 作成ジョブ enqueue
- Dashboardコメント更新やRun Resultコメント作成ジョブのenqueue

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
- root の `manifest.json` と DRC / ERC check 結果または skipped 状態の保存
- 期待 artifact ごとの `available` / `missing` / `failed` / `skipped` 状態保存
- path traversal や危険な entry 名の拒否
- `available` artifact の sha256 / size / content type の検証
- 展開後 `available` artifact の final bucket への保存
- artifact metadata / run summary / snapshot の DB 保存
- DRC / ERC のレビュー用明細の DB 保存

zip bundle 内の manifest は root の `manifest.json` を正本にする。
manifest の各 artifact は `type` と `status` を必須とする。
`available` artifact のみ `path`、`content_type`、`sha256`、`size_bytes` を必須とし、zip entry と一致検証する。
KiCanvas用の `kicad_project` / `kicad_schematic` / `kicad_pcb` は通常artifactと同じ保存モデルで扱うが、複数schematicを区別するため `source_path` または `logical_name` を持たせる。
KiCanvas用source artifactは、Actionが `project_dir` 配下の `.kicad_pro` / `.kicad_sch` / `.kicad_pcb` / `.kicad_wks` を、hash計算と同じexcludeルールを適用してbundleへ入れる前提で検証する。
manifest 未記載の zip entry は原則拒否し、root の `manifest.json` と仕様で許可した補助ファイルのみ例外として扱う。
import 成功済みの staging bundle は24時間以内、failed / timed_out run の staging bundle は7日後に削除対象とする。
final bucket の artifact は MVP では無期限保存とする。

private artifact 前提なので、配信時は以下を前提にする。

- artifact 専用 domain / subdomain
- 短命の署名付き URL または proxy 経由
- `Content-Security-Policy`
- 制限付き `Access-Control-Allow-Origin`
- `X-Content-Type-Options: nosniff`
- iframe sandbox

Web UIは `viewer-sources` APIから、KiCanvas、schematic PDF、PCB SVG/PDF、iBOM、BOM、fabrication downloadに必要な短命URLを取得する。
KiCanvas専用APIは作らず、viewer用途ごとのsourceを汎用レスポンスにまとめる。

## 8. 認証と認可

MVP では GitHub 中心の構成にする。

- ユーザーログイン: GitHub OAuth
- リポジトリ連携: GitHub App installation
- Action から SaaS への認証: BoardFlow API token

考慮点:

- token は hash のみ DB 保存
- token は repository 単位で発行し、plan API と BoardRun API では token に紐づく `installation_id + github_repository_id` と request repository を一致検証する
- token は `name`、`created_at`、`last_used_at`、`revoked_at` を持ち、revoke済みtokenは認可エラーにする
- `last_used_at` は認証成功時のみ更新する
- GitHub App webhook は MVP から含める
- installation 情報同期と権限確認を backend の責務にする
- Web UI の閲覧可否は GitHub 権限と揃える

GitHub App installation が解除済み、権限不足、または repository 不一致の場合、plan API は build/skip decision ではなく認可エラーを返す。
Plan API の per-project `decision: error` は、SaaS側で受け取った project payload の不正などproject単位のvalidation失敗に限定する。
Action側のlocal detection errorはPlan APIへ送られない前提で扱う。

Plan API では Issue 作成ジョブを enqueue しない。
Issue 作成は初回 `BoardRun.status = completed` 後に行う。
Issue はユーザーが発注などの区切りで close する運用を許容する。
BoardProject 設定 `recreate_issue_on_update` が有効で、active Issue が closed かつ `tree_hash` が変わった場合、backend は既存Issueをreopenせず新しいIssueを作成する。
MVPでは `recreate_issue_on_update` のデフォルトを `true` とする。
Issueタイトルや本文のユーザー編集は上書きせず、GitHub APIジョブ実行時にIssue/commentの404や削除を検出して必要な再作成を行う。
Run ResultコメントはMVPではERC/DRC error状態変化、新規error発生、errorからの復旧時のみ作成し、BOM差分やartifact状態差分だけでは作成しない。

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

- `.boardflow.yml` 探索
- project_path 決定
- tree hash / manifest 生成
- KiCad / iBOM 実行
- plan API 呼び出し
- build前の BoardRun 作成
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
- repository-scoped BoardFlow token の認可 test
- 初回 completed run 後に Issue 作成ジョブが enqueue される test
- 12時間超過した未完了 BoardRun が timed_out になる test
- 同一 `github_run_id + github_run_attempt` の BoardRun 作成が既存 run を返す test
- artifact 欠損が `available` / `missing` / `failed` / `skipped` として保存される test
- 複数 `kicad_schematic` artifact が `source_path` 付きで保存される test
- `viewer-sources` API がviewer別に短命URLと欠損状態を返す test
- DRC/ERC failed でも import 成功時は BoardRun completed になる test
- `fail-on-drc` 相当のCI失敗がBoardRun failedを発生させない test
- close済みIssueと `recreate_issue_on_update` の組み合わせ test
- Dashboardコメント削除時の再作成 test

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
- GitHub App と BoardFlow token のライフサイクル
- artifact proxy を置くか、署名付き URL のみで始めるか
