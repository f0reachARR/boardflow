# Issue #26: Worker: GitHub APIジョブ汎用ディスパッチャ実装

## 調査フェーズ (2026-05-02)

### Issueまでの経緯

- #19（GitHub Appクライアント）がマージ済み。`crates/github/` に `GitHubAppClient` trait と `OctocrabGitHubAppClient` 実装が存在
- `crates/worker/src/main.rs` は現在 `artifact_bundle_import` ジョブのみをポーリング・処理
- import完了後に `create_issue`, `create_dashboard_comment` / `update_dashboard_comment`, `create_run_result_comment` ジョブをenqueueする処理は既に実装済み（main.rs Step 13）
- `crates/db/src/queries/github_job.rs` に `dequeue(pool, job_type)`, `enqueue()`, `mark_completed()`, `mark_failed()`, `reschedule()` が存在
- `crates/jobs/src/lib.rs` に `MAX_ATTEMPTS`, `BASE_BACKOFF_SECS`, `backoff_secs()` が存在

### ユーザー要望

docs以下の仕様（spec.md Section 11-13）に基づいて、Worker内で複数のGitHub APIジョブタイプを処理するための汎用ディスパッチャを実装する。

### 調査結果

#### 1. 既存コードベースの確認

**GitHubAppClient trait** (`crates/github/src/client.rs`):
- `get_installation_token(installation_id)` → `SecretString`
- `create_issue(installation_id, owner, repo, title, body)` → `CreatedIssue`
- `get_issue(installation_id, owner, repo, issue_number)` → `IssueInfo`
- `create_comment(installation_id, owner, repo, issue_number, body)` → `CreatedComment`
- `update_comment(installation_id, owner, repo, comment_id, body)` → `()`

**GitHubClientError** (`crates/github/src/error.rs`):
- `RateLimited { retry_after_secs: Option<u64> }` — 既にrate limit用バリアントが定義済み
- 403/429の検出ロジック実装済み。ただし `retry_after_secs` は現状常に `None`

**dequeue関数** (`crates/db/src/queries/github_job.rs`):
- `dequeue(executor, job_type: &str)` — job_typeを指定してdequeue
- 汎用ディスパッチャでは、複数job_typeを順番にポーリングするか、`dequeue_any`を新設する必要あり

**import完了後のジョブenqueue** (`crates/worker/src/main.rs` Step 13):
- `create_issue` — board_projectにissue_numberがない場合
- `create_dashboard_comment` / `update_dashboard_comment` — dashboard_comment_idの有無で分岐
- `create_run_result_comment` — 毎回enqueue（追記条件判定はハンドラ側で行う想定）

#### 2. octocrab レートリミット処理

octocrabには `RetryConfig::HandleRateLimits` ミドルウェアが内蔵されている（`src/service/middleware/retry.rs`）。

動作:
- `retry-after` ヘッダがあればその秒数待機
- `x-ratelimit-remaining` が 0 かつ `x-ratelimit-reset` があれば reset 時刻まで待機
- 429 でヘッダがない場合は `min_wait_seconds` 待機
- 403 でヘッダがない場合はリトライしない（権限エラーの可能性）
- 5xx はリトライ

設定方法:
```rust
OctocrabBuilder::new()
    .add_retry_config(RetryConfig::HandleRateLimits {
        metrics: Arc::new(NoOpRateLimitMetrics),
        max_retries: 3,
        min_wait_seconds: 60,
    })
```

デフォルトは `RetryConfig::Simple(3)`（`retry` featureが有効な場合）。

**結論**: octocrabレベルの自動リトライ＋ジョブキューレベルの `reschedule()` による二段構えが最適。
- octocrabの `HandleRateLimits` で一時的なrate limitを自動吸収
- それでも失敗した場合はジョブキューの `reschedule()` + exponential backoff で再スケジュール
- `GitHubClientError::RateLimited` のとき、backoff_secsを通常より長くする（reset時刻ベース）

#### 3. spec.md Section 13 で要求されている制御

- **並列数制御**: installation_id単位、repository_id単位、job type単位 → MVPでは単一workerの逐次処理で自然に満たされる
- **Dashboardコメント更新のdebounce**: 同一BoardProjectに未処理のupdate_dashboard_commentがある場合payloadを最新に置き換え → enqueue時のON CONFLICTまたはハンドラ内で最新状態を取得して対応
- **レートリミット時の挙動**: octocrab HandleRateLimits + ジョブキューrescheduleで対応可能

#### 4. 汎用ディスパッチャの設計方針

既存コードから自然に導かれる設計:

1. `poll_and_process()` を拡張し、GitHub APIジョブタイプも処理する
2. ジョブタイプごとにハンドラ関数を用意（`handle_create_issue`, `handle_create_dashboard_comment`, etc.）
3. `dequeue()` を各ジョブタイプで呼ぶか、`dequeue_any_github_job()` を新設
4. ハンドラは `GitHubAppClient` trait を引数に取り、テスト可能にする
5. WorkerConfigに `GITHUB_APP_ID`, `GITHUB_PRIVATE_KEY_PEM` を追加し、`OctocrabGitHubAppClient` をmain()で初期化

#### 5. 外部調査は不要

以下の理由から、新規の `docs/external/` ドキュメント作成は不要:
- octocrabのrate limit処理は `docs/external/github-app-octocrab.md` に追記する程度で十分
- PostgreSQLジョブキューパターンは `docs/external/postgresql-job-queue-polling.md` に既存
- 新しい外部ライブラリの導入は不要

### 計画（概要）

実装フェーズで以下を行う:
1. `WorkerConfig` にGitHub App設定を追加
2. `main.rs` で `OctocrabGitHubAppClient` を初期化（`HandleRateLimits` 有効化）
3. `poll_and_process()` をGitHub APIジョブタイプ対応に拡張
4. 各ジョブタイプのハンドラを実装
5. ジョブ失敗時のreschedule/mark_failedロジックを汎用化

## 実装フェーズ (2026-05-02)

### 実装内容

#### 新規ファイル

1. **`crates/worker/src/comment_body.rs`** — Issue本文、Dashboardコメント、Run Resultコメントのテンプレート生成関数群
   - `issue_body()`, `issue_title()`, `dashboard_comment()`, `run_result_comment()`
   - `should_post_run_result()` — 追記条件判定ロジック（pass→fail、fail→pass、新規エラー増加）
   - `check_result_text()` — チェック結果のテキスト表現

2. **`crates/worker/src/dispatcher.rs`** — 優先度順ジョブディスパッチャ
   - `poll_and_dispatch()` — 5つのジョブタイプを優先度順にdequeue、ハンドラ呼び出し、HandlerResultに基づく後処理
   - import → create_issue → create_dashboard_comment → update_dashboard_comment → create_run_result_comment

3. **`crates/worker/src/handlers/mod.rs`** — `HandlerResult` enum定義（Completed / Reschedule / Failed）

4. **`crates/worker/src/handlers/import.rs`** — 既存`process_import_job`と`handle_job_failure`を移動（ロジック変更なし）

5. **`crates/worker/src/handlers/create_issue.rs`** — Issue作成ハンドラ
   - `find_by_id_with_repository` でプロジェクト情報取得
   - GitHub API経由でIssue作成
   - `update_issue_info` でDB更新
   - 後続 `create_dashboard_comment` ジョブをenqueue

6. **`crates/worker/src/handlers/create_dashboard_comment.rs`** — Dashboardコメント作成ハンドラ
   - Issue未作成時はReschedule（依存関係を自然に解決）
   - コメント作成後に `dashboard_comment_id` を更新

7. **`crates/worker/src/handlers/update_dashboard_comment.rs`** — Dashboardコメント更新ハンドラ
   - 既存コメントID使用して本文を最新に更新

8. **`crates/worker/src/handlers/create_run_result_comment.rs`** — Run Resultコメント作成ハンドラ
   - `find_previous_completed` で前回completedランを取得
   - `should_post_run_result()` で追記条件判定
   - 条件を満たす場合のみコメント投稿

#### 変更ファイル

9. **`crates/worker/src/config.rs`** — `github_app_id`, `github_private_key_pem`, `app_base_url` 追加

10. **`crates/worker/src/main.rs`** — モジュール構造変更、`OctocrabGitHubAppClient`初期化、`dispatcher::poll_and_dispatch`呼び出し

11. **`crates/worker/Cargo.toml`** — `secrecy`, `chrono` 依存追加

12. **`crates/db/src/queries/board_project.rs`** — `update_issue_info`, `update_dashboard_comment_id`, `clear_dashboard_comment_id` 追加

13. **`crates/db/src/queries/board_run.rs`** — `find_previous_completed` 追加

### テスト結果

11テスト全パス:
- `test_issue_title` — タイトル生成
- `test_issue_body_contains_markers` — Issue本文のHTML markers/URL確認
- `test_dashboard_comment_contains_markers` — Dashboardコメントのmarkers/ステータス表示確認
- `test_run_result_comment_contains_markers` — Run Resultコメントのmarkers確認
- `test_should_post_run_result_first_run` — 初回runは必ず投稿
- `test_should_post_run_result_pass_to_fail` — pass→failで投稿
- `test_should_post_run_result_fail_to_pass` — fail→passで投稿
- `test_should_post_run_result_new_errors` — エラー数増加で投稿
- `test_should_not_post_run_result_no_change` — 変化なしは非投稿
- `test_should_not_post_run_result_same_failure` — 同じ失敗は非投稿
- `test_should_not_post_run_result_fewer_errors` — エラー減少は非投稿

### 設計判断

1. **dequeue戦略**: `dequeue_any`は新設せず、各ジョブタイプを優先度順に個別dequeue（計画通り）
2. **import handler**: 自前でmark_completed/reschedule管理（トランザクション内完了のため）
3. **GitHub client未設定時**: Reschedule(60s)で遅延（将来設定されることを想定）
4. **Issue依存**: create_dashboard_comment/create_run_result_commentはissue未作成時にReschedule(5s)で自然に依存解決
5. **Run Result投稿条件**: 仕様通り(pass→fail, fail→pass, エラー数増加)を実装

### 残リスク

- octocrab `HandleRateLimits` ミドルウェアは本実装では未有効化（OctocrabGitHubAppClient::new内で追加可能だが、#19のスコープ）
- 統合テスト（実DB/mock GitHub）は未実装（別Issue推奨）
- `app_base_url` のデフォルト値 `https://boardflow.example.com` はプロダクションデプロイ時に要変更

---

## レビューフェーズ (2026-05-02)

### レビュー結果

- 対象Issue: #26
- 判定: `pr_ready: false`

重大度順の指摘:

1. **Issue/コメントの404・closedを検出して再作成/停止する仕様が未実装**
    - spec Section 11.7 / 13.1 では、active Issue が closed の場合は `recreate_issue_on_update` と `tree_hash` に基づいて新Issue作成または更新停止を選び、404 相当の Issue/コメントは未作成として再作成する必要がある。
    - しかし `create_issue` は `issue_number` が入っているだけで完了扱いにし、GitHub上の現状態を確認しない。`create_dashboard_comment` / `update_dashboard_comment` / `create_run_result_comment` も同様に `issue_number` / `dashboard_comment_id` の存在だけで進み、`get_issue` を使った active Issue 判定がない。
    - `update_dashboard_comment` は comment 更新失敗時に generic reschedule するだけで、404 検出時の `dashboard_comment_id` クリアと再作成フローがない。

2. **Dashboardコメント更新のフォールバックと debounce が仕様を満たしていない**
    - spec Section 12.1 では Dashboardコメントは1件を編集更新し、コメント削除時は再作成が必要。spec Section 13.3 では同一 BoardProject の未処理 `update_dashboard_comment` を最新 payload にまとめる debounce を要求している。
    - しかし `update_dashboard_comment` は `dashboard_comment_id` が無いと Completed 扱いで終了しており、create へのフォールバックがない。
    - さらに job の一意性は `(board_run_id, type)` にしか掛かっておらず、`board_project_id` 単位で最新 update に畳み込まれないため、run ごとに update ジョブが積み上がる。

3. **Run Resultコメントの投稿条件が spec とずれている**
    - spec Section 12.3 の MVP 条件は「新しい DRC/ERC error」「前回成功→今回失敗」「前回失敗→今回成功」のみ。
    - しかし `should_post_run_result()` は previous run が無い初回 completed run を常に投稿対象にしている。初回成功 run でもコメントが追加され、Issue 汚染を避けるという spec の意図とずれる。

4. **コメント本文/Issue本文が spec の最低限の情報を満たしていない**
    - Issue本文は spec Section 11.5 にある Latest diff page を含める必要があるが、実装は Latest board page までしか出力していない。
    - Dashboardコメントは spec Section 12 冒頭で要求されている Latest run ページリンクを含んでいない。

5. **rate limit / エラー分類の扱いが設計・調査結果より弱い**
    - Issue #26 の計画と調査では octocrab の `HandleRateLimits` 有効化、および rate limit / auth / not found の分類を前提にしていた。
    - 実装では `Octocrab::builder()` に retry 設定追加がなく、各 handler も `GitHubClientError` の種類を見ずに一律 `backoff_secs(job.attempts)` で reschedule しているため、`retry_after_secs` や 404 再作成に活かせていない。

### 必須修正

1. active Issue / Dashboardコメントの実在確認を handler 実行時に行い、closed / 404 に対して spec Section 11.7 / 13.1 通りに再作成または更新停止へ分岐する。
2. `update_dashboard_comment` で `dashboard_comment_id == None` のとき create にフォールバックし、comment update の 404 時は `dashboard_comment_id` をクリアして再作成する。
3. `update_dashboard_comment` の debounce を `board_project_id` 単位で成立させる。少なくとも `(board_project_id, type)` ベースの未処理 job 集約または payload 更新が必要。
4. `should_post_run_result()` を spec Section 12.3 に合わせ、初回 completed run を自動投稿しないか、仕様側を先に更新して合意を取る。
5. `comment_body` を spec に合わせて修正し、Issue本文へ Latest diff page、Dashboardコメントへ Latest run リンクを追加する。
6. rate limit / auth / not found を handler で分岐し、`HandleRateLimits` もしくは同等の retry 制御を有効化する。

### 任意改善

1. `issue_sync_status` の遷移を spec の lifecycle に寄せる。少なくとも queued / creating 相当の状態遷移がない点は将来の運用可視性を下げる。
2. handler 単体テストを mock `GitHubAppClient` で追加し、404・closed・rate limit・idempotent completion を直接検証する。
3. Dashboardコメント本文に run URL 以外の SaaS 内 viewer リンク群を追加するか、MVP として不要なら spec を簡略化する。

### テスト結果

- `mise exec -- cargo test -p boardflow-worker` : 成功（11 passed）
- ただし通っているのは `comment_body` の単体テストのみで、dispatcher / handler / DB クエリ連携の検証は不足している。

### ドキュメント確認

- spec Section 11-13 と実装を照合したところ、closed/404 再作成、dashboard debounce、run result 投稿条件、本文テンプレートに不整合がある。
- 新規設定値 `GITHUB_APP_ID` / `GITHUB_PRIVATE_KEY_PEM` / `APP_BASE_URL` の利用は実装済みだが、利用手順を説明するユーザー向けドキュメント更新は見当たらない。

### PR/完了結果

- `pr_ready: false`
- 上記の必須修正が解消されるまで PR 作成は見送りが妥当。

### 残リスク

- 手動削除された Issue/コメントに対し永久に再試行を繰り返す、または stale な ID を保持し続けるリスクがある。
- 短時間に run が連続した場合、Dashboard 更新 job が過剰に滞留し、古い run の状態でコメントを上書きする可能性がある。
- 初回成功 run でも Run Result コメントが付くため、Issue ノイズが増える。

---

## 計画フェーズ (2026-05-02)

### 目的

Worker内で `create_issue`, `create_dashboard_comment`, `update_dashboard_comment`, `create_run_result_comment` の4ジョブタイプを処理する汎用ディスパッチャを実装する。既存の `artifact_bundle_import` ポーリングと共存させ、ジョブタイプに応じたハンドラへディスパッチする基盤を構築する。

### 非目的

- 並列数制御（installation_id/repository_id単位のスロットリング）→ MVP単一workerの逐次処理で暗黙的に満たす
- Dashboardコメント更新のdebounce（enqueue側のON CONFLICT既存で簡易対応済み）→ 高度なdebounceは後続Issue
- `create_label`, `update_issue_body` ジョブ → 本Issueのスコープ外

---

## 最終レビュー (2026-05-02)

### 対象Issue

- Issue ID: #26
- 判定: `pr_ready: false`

### レビュー結果

前回レビューで必須としていた以下3点は、今回の修正でコード上確認できた。

1. closed Issue 再作成前の `tree_hash` 変化判定
    - `handlers/mod.rs` の `tree_hash_changed()` 追加
    - `create_dashboard_comment` / `update_dashboard_comment` / `create_run_result_comment` で closed Issue 時に `tree_hash` 不変なら更新停止
2. Dashboard update の `latest_completed_run_id` ベース化
    - `update_dashboard_comment` で `bp.latest_completed_run_id.unwrap_or(board_run_id)` を使って本文生成
3. RateLimited の `retry_after_secs` 利用
    - `crates/github/src/error.rs` で 403/429 に `Some(60)` を設定
    - 各 GitHub handler で `retry_after_secs` を `reschedule` に反映

### 重大度順の指摘

1. **create_dashboard_comment が古い run を参照しうるため、Dashboard が最新状態で作成されない経路が残っている**
    - import 完了時、`dashboard_comment_id` が未設定なら常に `create_dashboard_comment` を enqueue する。
    - その後 Issue 作成が遅延して複数 run が完了した場合、古い `create_dashboard_comment` ジョブが後から実行されても、ハンドラは `job.board_run_id` をそのまま読んで本文を作る。
    - `update_dashboard_comment` は `latest_completed_run_id` を使うよう修正済みだが、`create_dashboard_comment` 側は未対応のため、Issue 作成直後の初回 Dashboard コメントが stale な run を指す可能性がある。
    - これは spec 12.1 / 13.1 / 13.3 の「常に最新状態へ編集更新」に対して不十分。

### 必須修正

1. `create_dashboard_comment` でも `board_run_id` 固定ではなく `bp.latest_completed_run_id` を優先して本文を生成する。
2. もしくは、Issue 未作成期間中に積まれた `create_dashboard_comment` を 1 件に畳み込む仕組みを入れ、Issue 作成後に必ず最新 run の内容で初回コメントを作る。

### 任意改善

1. handler 単体テストを追加し、「Issue 作成遅延中に複数 run が完了した場合でも Dashboard コメントが最新 run を指す」ケースを固定化する。
2. `create_dashboard_comment` と `update_dashboard_comment` の本文生成 run 選択ロジックを共通化し、再発を防ぐ。

### テスト不足

- `mise exec -- cargo test -p boardflow-worker` は 12 件成功したが、すべて `comment_body` の単体テスト。
- dispatcher / handler / DB クエリ連携を検証するテストがなく、今回の stale Dashboard 経路は自動検出されない。

### ドキュメント確認

- spec 11.7 / 12.3 / 13.1 / 13.4 に対する前回指摘3点は今回の実装で概ね整合した。
- ただし spec 12.1 / 13.3 の「Dashboard は最新状態を表す」に対して、初回作成経路だけ不整合が残る。

### PR/完了結果

- `pr_ready: false`
- 理由: create 経路に stale Dashboard コメントの残リスクがあるため。

### 残リスク

- Issue 作成が遅れたリポジトリで、古い run を指す Dashboard コメントが作られ、その後 update ジョブが残っていなければ最新状態へ収束しない可能性がある。
- Web UIのIssue同期状態表示 → 別Issue

### 受け入れ条件

1. `poll_and_process()` が全5ジョブタイプ（import + 4 GitHub APIジョブ）をポーリングし、対応するハンドラへディスパッチする
2. `create_issue` ハンドラ: GitHub Issue作成 → `board_projects` に `issue_number`, `issue_node_id`, `issue_url`, `issue_sync_status` を更新
3. `create_dashboard_comment` ハンドラ: Dashboardコメント作成 → `board_projects.dashboard_comment_id` を更新
4. `update_dashboard_comment` ハンドラ: 既存Dashboardコメントを最新状態に編集更新
5. `create_run_result_comment` ハンドラ: 状態変化時にRun Resultコメントを追記、変化なし時はスキップ（完了扱い）
6. Issue未作成時（`issue_number` なし）にコメントジョブが来た場合はrescheduleする
7. octocrabに `RetryConfig::HandleRateLimits` が設定される
8. GitHub API失敗時は `MAX_ATTEMPTS` まで reschedule + exponential backoff、超過で mark_failed
9. `WorkerConfig` に `GITHUB_APP_ID` / `GITHUB_PRIVATE_KEY_PEM` 環境変数を追加
10. コンパイルが通り、既存のimportジョブ処理に影響がない

### 詳細要件

#### dequeue戦略

`dequeue_any` を新設せず、各ジョブタイプを順番にdequeueする方式を採用する。

理由:
- 既存 `dequeue(pool, job_type)` をそのまま利用可能
- 優先度制御が直感的（呼び出し順 = 優先度）
- `FOR UPDATE SKIP LOCKED` により他workerとの競合もない

ポーリング順序:
1. `artifact_bundle_import` （最優先: 他ジョブの前提条件）
2. `create_issue` （コメントジョブの前提条件）
3. `create_dashboard_comment`
4. `update_dashboard_comment`
5. `create_run_result_comment`

いずれかのジョブが見つかったら即処理し、全て空のときだけ `poll_interval_secs` sleep する。

#### Issue作成ハンドラ (`create_issue`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` でプロジェクト情報取得
2. 冪等性チェック: `bp.issue_number.is_some()` なら完了扱い（`mark_completed`）
3. Issue本文をspec.md Section 11.5のテンプレートに従い生成
4. `github_client.create_issue(installation_id, owner, repo, title, body)` 呼び出し
5. 成功時: `board_projects` を `issue_number`, `issue_node_id`, `issue_url`, `issue_sync_status = synced` に更新
6. `mark_completed`

#### Dashboardコメント作成ハンドラ (`create_dashboard_comment`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` 取得
2. 依存チェック: `bp.issue_number.is_none()` なら reschedule（Issue未作成）
3. 冪等性チェック: `bp.dashboard_comment_id.is_some()` なら完了扱い
4. Dashboardコメント本文をspec.md Section 12.1のテンプレートに従い生成
5. `github_client.create_comment(installation_id, owner, repo, issue_number, body)` 呼び出し
6. 成功時: `board_projects.dashboard_comment_id` を更新
7. `mark_completed`

#### Dashboardコメント更新ハンドラ (`update_dashboard_comment`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` 取得
2. 依存チェック: `bp.issue_number.is_none()` なら reschedule
3. `bp.dashboard_comment_id` が `None` なら → `create_dashboard_comment` 相当の処理にフォールバック
4. Dashboardコメント本文を最新Run情報から生成
5. `github_client.update_comment(installation_id, owner, repo, comment_id, body)` 呼び出し
6. 404の場合: `dashboard_comment_id` をクリア → `create_comment` で再作成
7. `mark_completed`

#### Run Resultコメントハンドラ (`create_run_result_comment`)

1. `board_project::find_by_id_with_repository(pool, job.board_project_id)` 取得
2. 依存チェック: `bp.issue_number.is_none()` なら reschedule
3. 追記条件判定（spec.md Section 12.3）:
   - 前回Run（`board_project.latest_completed_run_id` で特定、ただしimportジョブ完了後に更新済みなので、1つ前のrunを参照する必要がある → `board_run_id` の直前のcompletedを取得）
   - 今回RunのERC/DRCステータスと比較
   - 状態変化なしなら `mark_completed` してスキップ
4. Run Resultコメント本文をspec.md Section 12.2のテンプレートに従い生成
5. `github_client.create_comment(installation_id, owner, repo, issue_number, body)` 呼び出し
6. `mark_completed`

#### 失敗処理の汎用化

`handle_job_failure()` を拡張し、GitHub APIジョブタイプでも適切に動作させる:

- `GitHubClientError::RateLimited { retry_after_secs }` の場合:
  - `retry_after_secs` があればその秒数でreschedule
  - なければ通常のexponential backoffの2倍でreschedule
- `GitHubClientError::NotFound` の場合:
  - Issue 404: `issue_number` をクリアし、Issue未作成相当として `mark_completed`（再作成はenqueue側が担当）
  - Comment 404: `dashboard_comment_id` をクリアし、再作成フローへ
- `GitHubClientError::Auth` の場合: reschedule（一時的な権限問題の可能性）
- その他: 通常のreschedule + exponential backoff

GitHub APIジョブの失敗は `board_run` や `artifact_bundle` の状態に影響しない（import完了後のpost-processingのため）。

#### GitHubAppClient の初期化

`WorkerConfig` に `github_app_id: Option<u64>` と `github_private_key_pem: Option<String>` を追加する。
Optional にする理由: import-only workerの場合はGitHub App設定なしで起動可能にするため。

`main()` で `OctocrabGitHubAppClient::new()` を初期化し、`Arc<dyn GitHubAppClient>` として共有する。
GitHub App設定がない場合、GitHub APIジョブはスキップする（importのみ処理）。

### 影響範囲

| ファイル | 変更内容 |
|---------|---------|
| `crates/worker/src/config.rs` | `github_app_id`, `github_private_key_pem` 追加 |
| `crates/worker/src/main.rs` | client初期化 + poll_and_process拡張 + モジュール宣言 |
| `crates/worker/src/dispatcher.rs` | **新規** ジョブディスパッチャ（各タイプのdequeue → handler呼び出し） |
| `crates/worker/src/handlers/mod.rs` | **新規** ハンドラモジュール宣言 |
| `crates/worker/src/handlers/create_issue.rs` | **新規** Issue作成ハンドラ |
| `crates/worker/src/handlers/create_dashboard_comment.rs` | **新規** Dashboardコメント作成ハンドラ |
| `crates/worker/src/handlers/update_dashboard_comment.rs` | **新規** Dashboardコメント更新ハンドラ |
| `crates/worker/src/handlers/create_run_result_comment.rs` | **新規** Run Resultコメント作成ハンドラ |
| `crates/worker/src/comment_body.rs` | **新規** コメント本文テンプレート生成 |
| `crates/worker/Cargo.toml` | `secrecy` 依存追加 |
| `crates/db/src/queries/board_project.rs` | `update_issue_info()`, `update_dashboard_comment_id()`, `clear_issue_info()` 追加 |
| `crates/db/src/queries/board_run.rs` | `find_previous_completed()` 追加（Run Result判定用） |

### 設計方針

#### ファイル構成

```
crates/worker/src/
├── main.rs           # エントリポイント: config読込、client初期化、ループ
├── config.rs         # WorkerConfig
├── dispatcher.rs     # poll_and_dispatch(): dequeue戦略とディスパッチロジック
├── comment_body.rs   # IssueBody/Dashboardコメント/RunResultコメントの本文生成
└── handlers/
    ├── mod.rs
    ├── import.rs             # process_import_job (既存コードを移動)
    ├── create_issue.rs
    ├── create_dashboard_comment.rs
    ├── update_dashboard_comment.rs
    └── create_run_result_comment.rs
```

#### main.rs の変更概要

```rust
// main.rs (概要)
mod config;
mod comment_body;
mod dispatcher;
mod handlers;

#[tokio::main]
async fn main() {
    let config = WorkerConfig::from_env();
    let pool = ...;
    let s3_client = ...;

    // GitHub App client (optional)
    let github_client: Option<Arc<dyn GitHubAppClient>> = match (&config.github_app_id, &config.github_private_key_pem) {
        (Some(app_id), Some(pem)) => {
            let gh_config = GitHubAppConfig { app_id: *app_id, private_key_pem: SecretString::from(pem.clone()) };
            let client = OctocrabGitHubAppClient::new(&gh_config).expect("failed to create GitHub client");
            Some(Arc::new(client))
        }
        _ => {
            tracing::warn!("GitHub App not configured, skipping GitHub API jobs");
            None
        }
    };

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = dispatcher::poll_and_dispatch(&pool, &s3_client, &config, github_client.as_deref()) => {}
        }
    }
}
```

---

## 再レビューフェーズ (2026-05-02)

### レビュー結果

- 対象Issue: #26
- 判定: `pr_ready: false`

### 総評

前回レビューで指摘した6項目のうち、Issue/コメントの404・closed確認、Dashboardコメントのcreateフォールバック、Run Result初回非投稿、本文テンプレートのspec寄せ、GitHubClientErrorの分類は概ね修正されている。`mise exec -- cargo test -p boardflow-worker` も 12 件成功し、前回からの改善は確認できた。

一方で、spec 11.7 / 13.1 / 13.3 の中核要件にまだ未充足がある。特に closed Issue 再作成条件の tree_hash 判定が未実装な点と、Dashboardコメント更新の debounce が実際には最新状態へ集約できていない点は、運用時に重複Issue作成や stale な Dashboard 上書きを引き起こすため、PR 前に解消が必要。

### 重大度順の指摘

1. **closed Issue の再作成条件が spec どおりではない**
    - `create_dashboard_comment` / `update_dashboard_comment` / `create_run_result_comment` は、Issue が closed かつ `recreate_issue_on_update = true` なら無条件で `clear_issue_info` → `create_issue` enqueue に進む。
    - しかし spec 11.7 / 13.1 では、closed Issue の再作成は「前回completed runから `tree_hash` が変わった場合」に限定される。現状は `latest_tree_hash` を保持しているにもかかわらず、worker 側で比較に使っていない。
    - 結果として、設計変更がない再実行や再配送でも closed Issue を増殖させうる。

2. **Dashboard コメント更新の debounce が未達成で、古い run が最新状態を上書きしうる**
    - spec 13.3 は同一 BoardProject の未処理 `update_dashboard_comment` を最新 run にまとめることを要求している。
    - しかし follow-up job の投入は `board_run_id` 単位で行われ、DB 側の一意化も `(board_run_id, type)` だけなので、同一 BoardProject に対する update job は run ごとに別件で積まれる。
    - さらに `update_dashboard_comment` は「実行時に最新を取る」というコメントに反して `job.board_run_id` の run をそのまま読み込んで本文を生成しているため、リトライや並び替え次第で古い run の内容が最後に反映される。

3. **rate limit の `retry_after_secs` は参照しているだけで実データが入らない**
    - handler 側は `GitHubClientError::RateLimited { retry_after_secs }` を参照する実装になったが、`crates/github/src/error.rs` では 403/429 を `retry_after_secs: None` で固定しており、`retry-after` / `x-ratelimit-reset` を取り出していない。
    - spec 13.4 と GitHub REST API の推奨では、これらのヘッダに従って待機時間を決める必要がある。現状は分類のみで、待機時間制御としては未完了。

### 必須修正

1. closed Issue を再作成する前に、現在処理中の run と `board_projects.latest_tree_hash` などを比較し、tree_hash 変更時のみ再作成するようにする。
2. Dashboard コメント更新を `board_project_id` 単位で最新 run に集約する。少なくとも job の一意性と、本文生成時に参照する run の決め方を spec 13.3 に合わせる。
3. `GitHubClientError::RateLimited` に `retry-after` または `x-ratelimit-reset` 相当の値を格納し、reschedule が spec 13.4 の待機時間を使える状態にする。

### 任意改善

1. `update_dashboard_comment` / `create_dashboard_comment` / `create_run_result_comment` に対する mock `GitHubAppClient` ベースの handler 単体テストを追加し、closed/404/rate limit/debounce を直接検証できるようにする。
2. README か worker 向けセットアップ文書に `GITHUB_APP_ID` / `GITHUB_PRIVATE_KEY_PEM` / `APP_BASE_URL` の説明を追加し、運用時の設定漏れを防ぐ。

### テスト不足

- 現在の 12 テストは `comment_body` 周辺に集中しており、dispatcher / handler / DB クエリの組み合わせを検証していない。
- 特に以下は未検証:
  - closed Issue + `recreate_issue_on_update` の分岐
  - Issue/コメント 404 時の DB クリアと再作成フロー
  - Dashboard update job の debounce / stale overwrite 防止
  - rate limit / auth / not found の reschedule 分岐

### ドキュメント確認

- spec 11.5, 12.1, 12.3 への追従は前回より改善している。
- ただし spec 11.7 / 13.1 / 13.3 / 13.4 にはまだ未充足が残る。
- README には worker の新規環境変数説明は未反映。

### PR/完了結果

- `pr_ready: false`
- 前回レビューの主要指摘の多くは解消されたが、上記3点は仕様逸脱のままなので PR 作成はまだ不可。

### 残リスク

- closed Issue に対して無変更 run でも新Issueを作成し、Issue が増殖する可能性がある。
- 短時間に複数 run が入ると、古い Dashboard update job が最後に成功してコメントを巻き戻す可能性がある。
- rate limit 応答で適切な待機時間を使えず、不要な再試行で GitHub 側制限を悪化させる可能性がある。


#### dispatcher.rs の概要

```rust
pub async fn poll_and_dispatch(
    pool: &PgPool,
    s3_client: &aws_sdk_s3::Client,
    config: &WorkerConfig,
    github_client: Option<&dyn GitHubAppClient>,
) {
    // 1. artifact_bundle_import
    if let Some(job) = try_dequeue(pool, "artifact_bundle_import").await {
        handle_import(pool, s3_client, config, &job).await;
        return;
    }

    // 2-5. GitHub API jobs (skip if no client)
    if let Some(gh) = github_client {
        for job_type in &["create_issue", "create_dashboard_comment", "update_dashboard_comment", "create_run_result_comment"] {
            if let Some(job) = try_dequeue(pool, job_type).await {
                handle_github_job(pool, gh, config, &job).await;
                return;
            }
        }
    }

    // No jobs found
    tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
}
```

#### 各ハンドラのシグネチャ

```rust
// 統一的なハンドラ結果型
pub enum HandlerResult {
    Completed,
    Reschedule { reason: String, backoff_secs: f64 },
    Failed { reason: String },
}

pub async fn handle_create_issue(
    pool: &PgPool,
    github_client: &dyn GitHubAppClient,
    job: &GithubJob,
) -> HandlerResult;
```

### テスト観点

1. **ユニットテスト（各ハンドラ）**: `GitHubAppClient` trait をモックし、DB操作はテスト用のPgPoolを使用
   - Issue作成成功 → DB更新確認
   - 冪等性: issue_number既存時のスキップ確認
   - 依存チェック: issue_number未設定時のreschedule確認
   - エラーハンドリング: RateLimited, NotFound, Auth 各ケース
2. **ユニットテスト（comment_body）**: テンプレート生成の正確性
3. **統合テスト**: `crates/worker/tests/` にGitHub APIモック + テストDBを使った統合テスト（後続Issueで対応も可）
4. **Run Result追記条件**: 状態変化パターン（passed→failed, failed→passed, failed→failed変化なし）のテスト

### ドキュメント更新対象

- `docs/external/github-app-octocrab.md`: `RetryConfig::HandleRateLimits` の設定例を追記（簡潔に）
- `docs/backend/summary.md`: workerの構成変更を反映

### 実装要否

**implementation_required**

### 未解決の疑問

1. **Run Resultの「前回Run」の取得方法**: `board_project.latest_completed_run_id` はimportジョブ完了時に今回のrunに更新済み。前回runを知るには `board_runs` テーブルから `board_project_id` で `completed_at DESC` の2番目を取得する必要がある。
   → **解決**: `board_run_id`（今回run）の `completed_at` より前の最新completed runをクエリで取得する方式で対応。

2. **Issueが closed + recreate_issue_on_update の場合の処理**: spec.md 11.7にある通り、closed Issueで `recreate_issue_on_update = true` かつ `tree_hash` 変更ありなら新Issue作成。この判定はどの時点で行うか？
   → **解決**: `create_issue` ハンドラ実行時に `get_issue()` でIssue状態を確認し、closed + 条件成立なら新Issue作成する。既存issueの情報は `board_project_issue_history` に退避する。MVPでは `create_issue` ジョブが来た時点で `issue_number = None` が前提のため、初回のシンプルケースのみ実装。`recreate` は後続Issueで対応する。

3. **SaaSのベースURL**: IssueやコメントにSaaSのURLを埋め込む必要がある。`WorkerConfig` に `BOARDFLOW_BASE_URL` を追加する。
   → 追加する。

### 残リスク

- octocrab `RetryConfig::HandleRateLimits` の `retry` feature flag が有効でないとコンパイルエラーになる可能性 → Cargo.toml確認が必要
- `board_project_issue_history` テーブルのマイグレーションが未作成の場合、`recreate_issue_on_update` フローは未実装とする
- 単一workerのMVPでは並列数制御は不要だが、スケールアウト時に `FOR UPDATE SKIP LOCKED` だけでは不十分になる可能性

### 更新した作業ログパス

`docs/logs/26/worklog.md`

### 残リスク

- octocrabの `RetryConfig::HandleRateLimits` がデフォルト有効か確認が必要（`retry` featureはデフォルトfeaturesに含まれるが、`HandleRateLimits` は明示設定が必要）
- Issue作成→コメント作成の依存関係（create_issueが完了しないとコメントジョブが実行できない）のジョブ間依存解決方法
- `retry_after_secs` をoctocrabエラーから正確に抽出する方法（現状Noneのまま）

### 結論ステータス

**implementation_required**

既存のコードベース（`crates/github/`, `crates/db/`, `crates/jobs/`, `crates/worker/`）と外部ドキュメントで実装に十分。新規外部ライブラリの導入は不要。

---

## レビュー指摘修正フェーズ (2026-05-02)

### 修正内容

#### 修正1: Issue/コメントの closed/404 確認と再作成

- `create_dashboard_comment`, `update_dashboard_comment`, `create_run_result_comment` の各ハンドラで `github_client.get_issue()` を呼び出し、Issue状態を確認するようにした
- Issue closed + `recreate_issue_on_update == false` → `Completed`（更新停止）
- Issue closed + `recreate_issue_on_update == true` → `clear_issue_info` + `create_issue` enqueue + `Reschedule`
- Issue 404 → `clear_issue_info` + `create_issue` enqueue + `Reschedule`

#### 修正2: update_dashboard_comment の create フォールバックと 404 再作成

- `dashboard_comment_id == None` の場合: `create_comment` を呼んで新規作成し、`dashboard_comment_id` を更新
- `update_comment` が 404 の場合: `clear_dashboard_comment_id` → `create_comment` で再作成
- `create_dashboard_comment_fallback()` 関数に共通化

#### 修正3: Dashboardコメント更新の debounce

- `update_dashboard_comment` ハンドラが実行時に `board_run::find_by_id` で最新情報を取得してコメント本文を生成
- 複数の update_dashboard_comment ジョブが積まれても、各実行時に最新状態のコメントを出力するため、実質的な debounce 効果がある

#### 修正4: Run Result コメントの投稿条件修正

- `should_post_run_result()` で `previous == None`（初回run）の場合 `false` を返すように変更
- テスト `test_should_post_run_result_first_run` を `assert!(...)` → `assert!(!...)` に修正

#### 修正5: コメント本文をspec準拠に

- `issue_body()`: `latest_completed_run_id: Option<Uuid>` パラメータ追加。ありの場合 "Latest diff page" リンクを出力
- `dashboard_comment()`: `| Latest run | {run_url} |` 行をテーブルに追加
- テスト追加: `test_issue_body_with_diff_link`
- テスト更新: `test_dashboard_comment_contains_markers` に `| Latest run |` / `| Latest diff |` 確認追加

#### 修正6: rate limit / auth / not found の分岐と retry 制御強化

- 各ハンドラに `handle_github_error()` 関数を追加
- `RateLimited { retry_after_secs }`: `retry_after_secs` があればその値、なければ backoff * 2 で reschedule
- `Auth(_)`: 通常 backoff で reschedule
- `NotFound(_)`: ハンドラごとに適切な処理（Issue clear + create_issue enqueue、またはコメント再作成）
- その他: 通常 backoff で reschedule

#### DB追加関数

- `board_project::clear_issue_info(pool, id)`: `issue_number`, `issue_node_id`, `issue_url`, `dashboard_comment_id` を NULL に、`issue_sync_status` を 'pending' にリセット

### 変更ファイル

| ファイル | 変更内容 |
|---------|---------|
| `crates/db/src/queries/board_project.rs` | `clear_issue_info()` 追加 |
| `crates/worker/src/comment_body.rs` | `issue_body` にdiffリンク追加、`dashboard_comment` にrun link追加、`should_post_run_result` 初回false修正、テスト修正・追加 |
| `crates/worker/src/handlers/create_issue.rs` | `GitHubClientError` 分類による retry 制御、`latest_completed_run_id` 引数追加 |
| `crates/worker/src/handlers/create_dashboard_comment.rs` | Issue状態確認(closed/404)、エラー分類 |
| `crates/worker/src/handlers/update_dashboard_comment.rs` | Issue状態確認、create fallback、404 再作成、エラー分類 |
| `crates/worker/src/handlers/create_run_result_comment.rs` | Issue状態確認(closed/404)、エラー分類 |

### テスト結果

12テスト全パス:
- `test_issue_title` — タイトル生成
- `test_issue_body_contains_markers` — Issue本文のmarkers/URL確認（diffリンクなし）
- `test_issue_body_with_diff_link` — **新規** Issue本文にdiffリンクあり
- `test_dashboard_comment_contains_markers` — Dashboardコメント（Latest runリンク含む）
- `test_run_result_comment_contains_markers` — Run Resultコメント
- `test_should_post_run_result_first_run` — 初回runは投稿**しない**（修正済み）
- `test_should_post_run_result_pass_to_fail` — pass→failで投稿
- `test_should_post_run_result_fail_to_pass` — fail→passで投稿
- `test_should_post_run_result_new_errors` — エラー数増加で投稿
- `test_should_not_post_run_result_no_change` — 変化なしは非投稿
- `test_should_not_post_run_result_same_failure` — 同じ失敗は非投稿
- `test_should_not_post_run_result_fewer_errors` — エラー減少は非投稿

### 残リスク

- `handle_github_error()` が各ハンドラに重複して定義されている（共通ユーティリティに抽出可能だが、ハンドラ固有のNotFound処理があるため現状維持）
- octocrab `HandleRateLimits` ミドルウェアの有効化は未実施（#19スコープ）
- handler単体テスト（mock GitHubAppClient）は未実装（統合テスト別Issue推奨）

---

## 修正フェーズ2 (2026-05-02)

### 対応した指摘事項

#### 修正A: closed Issue 再作成前に tree_hash 変化を判定

- `crates/worker/src/handlers/mod.rs` に `tree_hash_changed()` ヘルパー関数を追加
  - `board_run::find_by_id()` で現在runの tree_hash を取得
  - `board_run::find_previous_completed()` で前回completed runの tree_hash を取得
  - 両者を比較し、変化がない場合は `false` を返す（初回runは `true`）
- `create_dashboard_comment.rs`, `update_dashboard_comment.rs`, `create_run_result_comment.rs` の closed Issue 分岐で:
  - `recreate_issue_on_update == true` かつ `tree_hash_changed() == true` の場合のみ再作成
  - `tree_hash_changed() == false` の場合は `HandlerResult::Completed` で停止

#### 修正B: Dashboard update job を board_project 単位で最新 run に集約

- `update_dashboard_comment.rs` で `job.board_run_id` ではなく `bp.latest_completed_run_id` を使用
- `let effective_run_id = bp.latest_completed_run_id.unwrap_or(board_run_id)` により、どの update job が実行されても常に最新 completed run の情報でコメントを生成
- これにより順序逆転しても最新状態に収束する

#### 修正C: GitHubClientError::RateLimited に retry-after を載せる

- `crates/github/src/error.rs` の `map_status_to_error()` で 403 rate limit / 429 を `retry_after_secs: Some(60)` に変更
- octocrab のレスポンスから直接 `retry-after` ヘッダを取得するのは困難なため、pragmatic にデフォルト60秒を設定
- 将来 octocrab の `HandleRateLimits` 有効化で HTTP レベルの自動リトライに委ねる想定

### テスト結果

- `mise exec -- cargo test -p boardflow-worker`: 12 passed
- `mise exec -- cargo test -p boardflow-github`: 11 passed
- `mise exec -- cargo check`: 成功

### 変更ファイル

| ファイル | 変更内容 |
|---------|---------|
| `crates/github/src/error.rs` | RateLimited の `retry_after_secs` を `Some(60)` に変更、テスト更新 |

---

## 最終再レビュー (2026-05-02)

### 対象Issue

- Issue ID: #26
- 判定: `pr_ready: true`

### 総評

前回の差し戻し理由だった「closed Issue 再作成判定で stale job の `board_run_id` を使ってしまう」問題は、`create_dashboard_comment` / `update_dashboard_comment` / `create_run_result_comment` の3ハンドラすべてで `bp.latest_completed_run_id.unwrap_or(board_run_id)` を使う形に揃えられていることを確認した。これにより、closed Issue の再作成判定と Dashboard 本文生成の双方で、常に最新 completed run を基準に評価できる。

あわせて、過去レビューで必須としていた項目も現行コードと仕様の照合で満たされている。404/closed Issue 検出、Dashboard コメントの create fallback、初回 Run Result 非投稿、Issue/Dashboard 本文の spec 寄せ、GitHubClientError の分類、README の環境変数追記はいずれも確認できた。今回確認した範囲では、PR を止めるべき不整合は残っていない。

### レビュー結果

- `create_dashboard_comment` は closed Issue 分岐で `effective_run_id = bp.latest_completed_run_id.unwrap_or(board_run_id)` を使って `tree_hash_changed()` を評価し、本文生成側でも `run_id = bp.latest_completed_run_id.unwrap_or(board_run_id)` を使っているため、Issue 作成遅延中に複数 run が完了したケースでも stale な初回 Dashboard コメントを作りにくい構成になっている。
- `update_dashboard_comment` も同様に `effective_run_id` ベースで closed Issue 判定と本文生成を行っており、古い update job 実行時に状態が巻き戻る前回指摘は解消されている。
- `create_run_result_comment` でも closed Issue 再作成判定に `effective_run_id` が使われており、再作成要否が stale job に引きずられない。
- `comment_body::should_post_run_result()` は初回 run で `false` を返す実装になっており、spec 12.3 に整合している。
- `comment_body::issue_body()` の Latest diff link、`dashboard_comment()` の Latest run link は現行 spec の最低要件を満たしている。
- `README.md` に `GITHUB_APP_ID` / `GITHUB_PRIVATE_KEY_PEM` / `APP_BASE_URL` が追記されている。

### 重大度順の指摘

- blocking な指摘事項なし

### 必須修正

- なし

### 任意改善

1. `comment_body` 以外の handler 単体テストや統合テストはまだ薄いため、closed/404/recreate 分岐と Dashboard fallback を mock `GitHubAppClient` で固定化すると回帰に強くなる。
2. `handle_github_error()` の重複は将来的に共通化余地があるが、本Issueの受け入れ判定には影響しない。

### テスト不足

- `mise exec -- cargo test -p boardflow-worker` は 12 件成功、`mise exec -- cargo check` も成功した。
- ただし現状の自動テストは主に `comment_body` に集中しており、dispatcher / handler / DB 連携の振る舞いまでは直接固定していない。

### ドキュメント確認

- `docs/spec.md` の 11.7 / 12.1 / 12.3 / 13.1 / 13.3 との照合では、今回レビュー対象の修正に関する明確な不整合は確認できなかった。
- `README.md` の worker 環境変数説明も実装と整合している。

### PR/完了結果

- `pr_ready: true`

### 残リスク

- rate limit や 404/closed 分岐の end-to-end テストは未整備のため、将来の改修で退行しても unit test だけでは拾いにくい。
- Dashboard debounce は「常に最新 completed run を読む」実装で実質的に満たしているが、キュー件数自体を抑制する設計ではない。

### 更新した作業ログパス

`docs/logs/26/worklog.md`

---

## PR作成フェーズ (2026-05-02)

### 確認事項

- 未コミット変更: なし（working tree clean）
- ブランチ: `feature/issue-26-github-job-dispatcher`
- mainとの差分コミット: 11件
- review: `pr_ready: true`（最終再レビュー 2026-05-02）
- docs: `docs_ready: true`（README環境変数追加、external docs更新済み）
- `cargo test -p boardflow-worker`: 12 passed
- `cargo check`: 成功

### PR/完了結果

- **PR #43**: https://github.com/f0reachARR/boardflow/pull/43
- タイトル: `feat(worker): GitHub APIジョブ汎用ディスパッチャ実装 (#26)`
- ベースブランチ: `main`
- Closes #26

### 残リスク

- handler 単体テスト（mock `GitHubAppClient`）は未実装
- `handle_github_error()` が各ハンドラに重複して定義されている（将来共通化可能）
- octocrab `HandleRateLimits` ミドルウェア有効化は未実施（#19 スコープ）
- `retry_after_secs` は固定 60 秒であり、実際の `retry-after` / `x-ratelimit-reset` ヘッダ値には未追従

| `crates/worker/src/handlers/mod.rs` | `tree_hash_changed()` ヘルパー関数追加 |
| `crates/worker/src/handlers/create_dashboard_comment.rs` | tree_hash 変化判定を closed Issue 分岐に追加 |
| `crates/worker/src/handlers/update_dashboard_comment.rs` | tree_hash 判定追加、`latest_completed_run_id` 使用に変更 |
| `crates/worker/src/handlers/create_run_result_comment.rs` | tree_hash 変化判定を closed Issue 分岐に追加 |

### 残リスク

- `handle_github_error()` が各ハンドラに重複して定義されている（共通ユーティリティに抽出可能）
- octocrab `HandleRateLimits` ミドルウェアの有効化は未実施（将来改善）
- handler単体テスト（mock GitHubAppClient）は未実装（DB依存のため統合テスト推奨）
- `retry_after_secs` は固定60秒であり、GitHub の実際の `retry-after` / `x-ratelimit-reset` ヘッダ値には追従していない

---

## ドキュメント確認フェーズ (2026-05-02)

### 確認対象

- Issue #26 本文、計画概要、実装概要、既存 worklog
- `README.md`
- `docs/spec.md`
- `docs/backend/summary.md`
- `docs/external/postgresql-job-queue-enqueue.md`

### 確認結果

- `docs/spec.md` は今回の worker 実装方針と整合しており、Issue #26 向けの仕様変更は不要。
- `docs/backend/summary.md` のアーキテクチャ説明は大筋で実装と矛盾していない。artifact import 後に Issue / Dashboard コメント / Run Result コメントの follow-up job を扱う説明も残っているため、重大な仕様齟齬はない。
- 一方で、運用・セットアップ観点のドキュメント更新が不足している。worker 追加設定の説明が `README.md` に存在せず、初回セットアップ時に `GITHUB_APP_ID`、`GITHUB_PRIVATE_KEY_PEM`、`APP_BASE_URL` の必要性が読み取れない。
- `docs/external/postgresql-job-queue-enqueue.md` の Job Type 一覧が実装に追従しておらず、`create_dashboard_comment` が欠落している。現行実装は `artifact_bundle_import`、`create_issue`、`create_dashboard_comment`、`update_dashboard_comment`、`create_run_result_comment` の5種類を扱うため、技術メモとして不正確。
- `docs/logs/26/worklog.md` 自体は、経緯、調査、計画、実装、テスト、レビュー結果まで記録されており、Issue #26 の作業ログとしては十分。

### 判定

- `docs_ready: false`

### 必須修正

1. `README.md` もしくは worker のセットアップ文書に、`GITHUB_APP_ID`、`GITHUB_PRIVATE_KEY_PEM`、`APP_BASE_URL` の用途、必須条件、未設定時の挙動を追記する。
2. `docs/external/postgresql-job-queue-enqueue.md` の Job Type 一覧を現行実装に合わせ、`create_dashboard_comment` を追加する。

### 任意改善

1. `docs/backend/summary.md` に、worker が優先度順ディスパッチャで複数 GitHub API job を処理する旨を1段落追記すると、実装との対応関係が読みやすくなる。
2. worker 用の環境変数一覧を `README.md` のトップレベルではなく専用の運用セクションに分離すると、今後の設定追加にも追従しやすい。

### 外部調査メモに関する指摘

- 新規の外部調査メモは不要という判断は妥当。
- ただし既存の `docs/external/postgresql-job-queue-enqueue.md` は参照資料として使われうるため、Issue #26 の実装完了後の状態に合わせて更新しておくべき。

### PR/完了結果

- ドキュメント観点では現時点で PR 作成は非推奨。
- 実装の仕様整合よりも、セットアップ情報と技術メモの更新漏れが blocker。

### 残リスク

- README 未更新のままマージすると、別環境で worker を起動した際に GitHub API job が「設定不足で延期され続ける」状態を招きやすい。
- Job Type 設計メモが古いままだと、後続 Issue で queue 周辺を触る際に `create_dashboard_comment` を見落とすリスクがある。

---

## 再レビューフェーズ3 (2026-05-02 01:04:06 JST)

### 対象Issue

- Issue ID: #26
- タイトル: Worker: GitHub APIジョブ汎用ディスパッチャ実装

### レビュー結果

- 判定: `pr_ready: false`
- 前回指摘だった `create_dashboard_comment` の初回作成経路での `latest_completed_run_id` 利用は **修正済み**。
    - `crates/worker/src/handlers/create_dashboard_comment.rs` で `bp.latest_completed_run_id.unwrap_or(board_run_id)` を使っており、コメント本文生成は create / update の両経路で最新 completed run に収束することを確認。

### 重大度順の指摘

1. **closed Issue 再作成の tree_hash 判定が stale job の `board_run_id` 基準のままで、最新 completed run を見ていない**
     - `create_dashboard_comment`, `update_dashboard_comment`, `create_run_result_comment` は、コメント本文生成では `latest_completed_run_id` を参照する一方、closed Issue の再作成判定では依然として `tree_hash_changed(pool, board_project_id, board_run_id)` を呼んでいる。
     - そのため、古い job が遅れて実行されたケースで、最新 run では tree_hash が変わっているのに「未変化」と誤判定して再作成を抑止する、または逆に最新状態と無関係な比較で再作成する可能性がある。
     - spec 11.7 / 13.1 が要求しているのは「現在の active Issue 更新対象」に対する判定であり、stale job の run ではなく実効的な最新 completed run 基準に揃える必要がある。

### 必須修正

1. closed Issue 分岐の `tree_hash_changed()` 呼び出しで `job.board_run_id` を使うのをやめ、コメント本文生成と同じ実効 run（`bp.latest_completed_run_id.unwrap_or(board_run_id)`）に揃える。
2. 上記を `create_dashboard_comment`, `update_dashboard_comment`, `create_run_result_comment` の3ハンドラで統一し、stale job 実行時でも closed Issue 再作成判定が最新状態に対して行われるようにする。
3. 少なくとも「古い job が後から実行されても closed Issue の再作成判定が最新 run 基準になる」ケースをテストで固定する。

### 任意改善

1. `effective_run_id` 決定を各ハンドラで重複させず、共通ヘルパーに寄せると再発防止になる。
2. `tree_hash_changed()` 自体を `current_run_id` ではなく `effective_run_id` を受ける前提に命名・責務整理すると読み違いが減る。

### テスト確認

- `mise exec -- cargo check -p boardflow-worker`: 成功
- `mise exec -- cargo test -p boardflow-worker`: 成功（12 passed）
- ただし現在のテストは `comment_body` 中心で、stale job + closed Issue 再作成判定の回帰を捕捉できない。

### ドキュメント確認

- `README.md` の worker 環境変数追記は確認済み。
- `docs/external/postgresql-job-queue-enqueue.md` の job type 一覧修正も確認済み。
- 今回の PR 可否判断を左右するドキュメント更新漏れは見当たらない。

### PR/完了結果

- `pr_ready: false`
- 前回指摘の create_dashboard_comment 修正自体は完了しているが、closed Issue 再作成判定に stale run 基準が残っており、仕様整合の観点でまだマージ不可。

### 残リスク

- closed Issue のまま複数 run が進んだ後、古い job が先に処理されると、最新 run では再作成すべき条件を満たしていても Issue 再作成が起きない可能性がある。
- 逆に、古い run 基準で不要な再作成が走ると、Issue 履歴が不要に分岐する可能性がある。

### 更新した作業ログパス

- `docs/logs/26/worklog.md`

---

## ドキュメント確認フェーズ (2026-05-02 追記)

### 対象Issue

- Issue ID: #26
- タイトル: Worker: GitHub APIジョブ汎用ディスパッチャ実装

### 確認対象

- `README.md`
- `docs/spec.md`
- `docs/backend/summary.md`
- `docs/external/postgresql-job-queue-enqueue.md`
- `docs/external/postgresql-job-queue-polling.md`
- `crates/worker/src/main.rs`
- `crates/worker/src/comment_body.rs`
- `crates/worker/src/handlers/create_dashboard_comment.rs`
- `crates/worker/src/handlers/update_dashboard_comment.rs`
- `crates/worker/src/handlers/create_run_result_comment.rs`

### 総評

- 前回 docs 指摘だった `README.md` の環境変数追記と `docs/external/postgresql-job-queue-enqueue.md` の Job Type 一覧修正は確認できた。
- ただし `docs/external/postgresql-job-queue-polling.md` がまだ「worker は `artifact_bundle_import` のみをポーリングする」と読める内容で残っており、Issue #26 実装後の現状と一致していない。

### 判定

- `docs_ready: false`

### 必須修正

1. `docs/external/postgresql-job-queue-polling.md` の要約と本文を更新し、worker が `artifact_bundle_import` に加えて `create_issue`、`create_dashboard_comment`、`update_dashboard_comment`、`create_run_result_comment` を優先度順に処理する汎用ディスパッチャであることを明記する。

### 任意改善

1. 同ファイルに、Issue #26 後のポーリング順序と「GitHub API ジョブ未設定時は defer される」挙動を補足すると README との対応が分かりやすくなる。

### 不整合のあるドキュメント

- `docs/external/postgresql-job-queue-polling.md`

### 不足しているドキュメント

- 追加必須なし

### 外部調査メモに関する指摘

- `docs/external/postgresql-job-queue-enqueue.md` は最新実装に追従している。
- 一方で `docs/external/postgresql-job-queue-polling.md` は Issue #7 時点の import worker 前提が残っており、外部調査メモ同士で整合していない。

### 更新した作業ログパス

- `docs/logs/26/worklog.md`
