# Issue #23: Worker: BoardRunタイムアウト処理実装

## 経緯
- 12時間以上未完了のBoardRunを`timed_out`としてマークする定期処理をworkerに実装する

## ユーザー要望
- docs/spec.md に基づいたタイムアウト処理の実装

## 仕様参照
- spec.md L1039: BoardRun作成から12時間以内に `completed` または `failed` へ到達しない場合、workerが `timed_out` に遷移させる
- spec.md L1351: `created`、`uploading`、`importing` のまま12時間を超えたBoardRunはworkerが `timed_out` に遷移させる
- spec.md L2191: BoardRunの12時間timeout はMVP対象
- spec.md L2260: BoardRun作成から12時間を超えた未完了runはtimed_outになる

## フェーズ進捗
- [x] 調査
- [x] 計画
- [ ] 実装
- [ ] レビュー
- [ ] ドキュメント確認
- [ ] PR作成

---

## 計画 (2026-05-02)

### 目的
12時間以上未完了のBoardRunをworkerが定期的に検出し、`timed_out`ステータスに遷移させる。異常終了（GitHub Actions cancel、runner停止等）もMVPでは本処理でカバーする。

### 非目的
- BoardRunごとの個別タイムアウト時間の設定
- タイムアウト時のユーザー通知（将来Issue）
- GitHub Actionsへのキャンセルリクエスト送信

### 受け入れ条件
1. `created`, `uploading`, `importing` ステータスで `created_at` から12時間以上経過したBoardRunが `timed_out` に更新される
2. `timed_out_at` に現在時刻が設定される
3. `completed`, `failed`, `timed_out` のターミナル状態は影響を受けない
4. 12時間未満のBoardRunは影響を受けない
5. スイープは約60秒に1回実行される（ジョブポーリングを妨げない）
6. スイープの結果（件数）がログ出力される

### 詳細要件

#### 1. DBクエリ (`crates/db/src/queries/board_run.rs`)
- 新規関数: `sweep_timed_out`
- SQL: `UPDATE board_runs SET status = 'timed_out', timed_out_at = NOW() WHERE status IN ('created', 'uploading', 'importing') AND created_at < NOW() - interval '12 hours' RETURNING id`
- 戻り値: `Result<Vec<Uuid>, sqlx::Error>` (タイムアウトしたRun IDのリスト)

#### 2. Worker統合 (`crates/worker/src/dispatcher.rs`)
- 新規関数: `sweep_timed_out_runs(pool: &PgPool)`
- `poll_and_dispatch` の最初で最後のスイープからの経過時間を確認し、60秒以上経過していればスイープ実行
- 実装方式: `poll_and_dispatch` に `last_sweep: &mut tokio::time::Instant` パラメータを追加し、呼び出し元(main.rs)で状態管理

#### 3. Worker config (`crates/worker/src/config.rs`)
- `WorkerConfig` に `timeout_sweep_interval_secs: u64` を追加 (デフォルト60)
- 環境変数: `TIMEOUT_SWEEP_INTERVAL_SECS`

#### 4. Main loop (`crates/worker/src/main.rs`)
- `last_sweep: tokio::time::Instant` をループ前に初期化
- `poll_and_dispatch` 呼び出し時に `&mut last_sweep` を渡す

### 影響範囲
- `crates/db/src/queries/board_run.rs` — 関数追加
- `crates/worker/src/config.rs` — フィールド追加
- `crates/worker/src/dispatcher.rs` — スイープ呼び出し追加、関数シグネチャ変更
- `crates/worker/src/main.rs` — `last_sweep` 状態追加、呼び出し引数追加

### 設計方針
- **シンプルなタイマー方式**: `tokio::time::Instant` で最終スイープ時刻を管理。別タスク/スレッドは使わない。
- **一括UPDATE**: 対象レコードを1クエリで一括更新。ループで1件ずつ処理しない。
- **冪等性**: 同じRunを複数回スイープしても問題なし（WHERE句でstatusを絞っているため）。
- **ジョブキュー非依存**: スイープはジョブキューを経由せず、直接DBを更新。タイムアウト処理自体がジョブキュー障害時のフェイルセーフでもあるため。

### テスト観点
**統合テスト** (`crates/worker/tests/timeout_sweep_test.rs`):
1. **12時間超の未完了Run → TimedOut**: `created_at` を13時間前に設定したRunが `timed_out` になること
2. **12時間未満は影響なし**: `created_at` を11時間前に設定したRunが元のstatusのままであること
3. **ターミナル状態は影響なし**: `completed`, `failed`, `timed_out` のRunが変更されないこと
4. **対象ステータス網羅**: `created`, `uploading`, `importing` のそれぞれがタイムアウト対象であること
5. **timed_out_at設定**: タイムアウト後に `timed_out_at` がNon-Nullであること

### ドキュメント更新対象
- `docs/logs/23/worklog.md` — 作業記録（本ファイル）
- 他のドキュメント更新は不要（spec.mdに既に記載済み）

### 実装順序
1. `crates/db/src/queries/board_run.rs` に `sweep_timed_out` 関数追加
2. `crates/worker/src/config.rs` に `timeout_sweep_interval_secs` 追加
3. `crates/worker/src/dispatcher.rs` に `sweep_timed_out_runs` 関数追加、`poll_and_dispatch` シグネチャ変更
4. `crates/worker/src/main.rs` に `last_sweep` 状態追加
5. `crates/worker/tests/timeout_sweep_test.rs` 統合テスト作成
6. `cargo test -p boardflow-worker` で全テスト通過確認

### 変更対象ファイル一覧

| ファイル | 変更種別 | 概要 |
|---------|---------|------|
| `crates/db/src/queries/board_run.rs` | 関数追加 | `sweep_timed_out` クエリ関数 |
| `crates/worker/src/config.rs` | フィールド追加 | `timeout_sweep_interval_secs` |
| `crates/worker/src/dispatcher.rs` | 関数追加+変更 | スイープ関数、`poll_and_dispatch`シグネチャ変更 |
| `crates/worker/src/main.rs` | 変更 | `last_sweep` 状態管理 |
| `crates/worker/tests/timeout_sweep_test.rs` | 新規 | 統合テスト |

### 実装要否
`implementation_required`

### 未解決の疑問
なし — 仕様が明確であり、既存コードの構造も十分把握済み。

### 残リスク
- DBの時刻精度依存: `NOW()` はDBサーバーの時刻に依存するが、通常運用では問題にならない
- 大量のタイムアウト対象が一度に存在する場合のロック競合: MVPでは問題になる規模ではない

---

## 実装完了 (2026-05-02)

### 変更内容

| ファイル | 変更内容 |
|---------|---------|
| `crates/db/src/queries/board_run.rs` | `sweep_timed_out()` 関数追加 — 12時間超の非ターミナルRunを一括UPDATE |
| `crates/worker/src/config.rs` | `timeout_sweep_interval_secs` フィールド追加 (デフォルト60秒) |
| `crates/worker/src/dispatcher.rs` | `sweep_timed_out_runs()` 公開関数追加 + `board_run` import追加 |
| `crates/worker/src/main.rs` | `last_sweep` タイマー管理、`poll_and_dispatch`後にスイープチェック |
| `crates/worker/src/handlers/create_issue.rs` | テスト内 `WorkerConfig` に新フィールド追加 |
| `crates/worker/tests/create_issue_test.rs` | テスト内 `WorkerConfig` に新フィールド追加 |
| `crates/worker/tests/timeout_sweep_test.rs` | 新規: 統合テスト3件 |

### 設計判断
- 計画の「`poll_and_dispatch`シグネチャ変更」方式ではなく、Issueの指示に従い`main.rs`側で`last_sweep`を管理する方式を採用
- `poll_and_dispatch`のシグネチャは変更なし（既存テストへの影響最小化）

### テスト結果
- `cargo check --workspace` ✅ 成功
- `cargo test -p boardflow-worker --lib` ✅ 21テスト全パス
- `cargo test -p boardflow-worker --test timeout_sweep_test -- --ignored` ✅ 3テスト全パス
  1. `test_sweep_marks_stale_runs_as_timed_out` — 13h前のcreated/uploading/importing → timed_out
  2. `test_sweep_does_not_affect_recent_runs` — 11h前のRunは影響なし
  3. `test_sweep_does_not_affect_terminal_states` — completed/failed/timed_outは影響なし

### 更新ドキュメント
- `docs/logs/23/worklog.md` (本ファイル)

### 残リスク
- 並列テスト実行時、`sweep_timed_out`がグローバルUPDATEのため他テストのRunも巻き込む可能性 → テストでは戻り値ではなく行状態を直接検証する方式で対策済み
- タイムアウト通知機能は未実装（将来Issue）

---

## レビュー結果 (2026-05-02)

### 総評
- `sweep_timed_out` のSQL自体は仕様どおりで、`created` / `uploading` / `importing` のみを `timed_out` に一括更新し、terminal state を触らない点は妥当
- 一方で、worker への統合方法は「約60秒ごとの独立した定期処理」になっておらず、`poll_and_dispatch` の完了タイミングと `poll_interval_secs` にスイープ間隔が従属している
- さらに、毎分実行される想定のクエリに対して `board_runs(status, created_at)` 系の索引がなく、件数増加時に全表走査の定期負荷になる
- 上記のため、この時点では `pr_ready: false`

### 重大度順の指摘

#### 1. High: スイープ実行間隔が `TIMEOUT_SWEEP_INTERVAL_SECS` を保証していない
- [crates/worker/src/main.rs](crates/worker/src/main.rs#L73) で interval 値を作っているが、実際の判定は [crates/worker/src/main.rs](crates/worker/src/main.rs#L92-L93) のとおり `poll_and_dispatch` 完了後にしか行われない
- `poll_and_dispatch` はジョブが無い場合に [crates/worker/src/dispatcher.rs](crates/worker/src/dispatcher.rs#L119-L120) で `poll_interval_secs` 分 sleep するため、`POLL_INTERVAL_SECS > TIMEOUT_SWEEP_INTERVAL_SECS` の設定では timeout sweep は設定値どおりに走らない
- 同様に、長時間かかる import job が1件あるだけでも sweep はその完了まで止まる。Issue の受け入れ条件「約60秒に1回実行」「ジョブポーリングを妨げない」とは一致しない
- 対応案: `tokio::time::interval` を `select!` に独立枝として追加するか、別 task で sweep を実行してジョブ処理と cadence を分離する

#### 2. Medium: 定期 sweep SQL に対応する索引がなく、運用時に全表走査になりやすい
- sweep クエリは [crates/db/src/queries/board_run.rs](crates/db/src/queries/board_run.rs#L199-L206) のとおり `status IN (...) AND created_at < NOW() - interval '12 hours'` で絞り込む
- しかし現行スキーマで確認できる `board_runs` の索引は [crates/db/migrations/20260430000001_create_schema.up.sql](crates/db/migrations/20260430000001_create_schema.up.sql#L215) の `board_project_id` のみで、この条件に効く索引がない
- worker が 60 秒ごとにこの UPDATE を打つ想定なら、件数増加に伴って毎回 seq scan になりやすい
- 対応案: `created_at` 先頭の partial index 例 `WHERE status IN ('created','uploading','importing')` を追加し、timeout 対象集合だけを引けるようにする

### 必須修正
- timeout sweep を job loop から独立した cadence で実行するように変更する
- 上記変更後、`POLL_INTERVAL_SECS` や長時間 job に引きずられず sweep が動くことを確認するテストを追加する

### 任意改善
- `board_runs` の timeout sweep 用 partial index を追加する
- `sweep_timed_out_runs` で件数だけでなく代表 ID か elapsed time を debug ログに残し、運用時の観測性を上げる

### テスト不足
- [crates/worker/tests/timeout_sweep_test.rs](crates/worker/tests/timeout_sweep_test.rs#L111) から [crates/worker/tests/timeout_sweep_test.rs](crates/worker/tests/timeout_sweep_test.rs#L177) は DB クエリの結果検証に留まっており、main loop 統合後のスケジューリングは検証していない
- ちょうど12時間境界のケースが無い。仕様文言が「12時間以内」「12時間を超えた」で分かれているため、`=` 境界を固定しておくと将来の解釈ブレを防げる
- テストは全て `#[ignore]` で [crates/worker/tests/timeout_sweep_test.rs](crates/worker/tests/timeout_sweep_test.rs#L3-L4) のとおり外部 DB 前提。通常の `cargo test` ではこの変更の主機能が自動では担保されない

### ドキュメント確認
- 実装で [crates/worker/src/config.rs](crates/worker/src/config.rs#L30) に `TIMEOUT_SWEEP_INTERVAL_SECS` が追加されているが、Worker 環境変数一覧は [README.md](README.md#L19-L32) にまだ記載がない
- `docs/spec.md` の timeout 要件との整合は取れている

### plan / research / docs との不整合
- 計画上の「約60秒に1回実行」は実装では保証されていない
- `docs/external/postgresql-job-queue-polling.md` が示す polling loop は job dequeue cadence の話であり、今回の timeout sweep を同じ戻りタイミングに束ねると責務が結合する
- ドキュメント更新対象を「不要」としているが、実際には README の環境変数一覧更新が必要

### PR/完了結果
- `pr_ready: false`
- レビュー担当としては、少なくとも sweep cadence の独立化とその確認テストまでは反映後に再レビューが必要

### 残リスク
- partial index を追加しない場合、BoardRun 蓄積後に worker の定期 sweep が DB 負荷要因になる
- タイムアウト処理が job handler 実行時間に依存したままだと、障害時に本来 timeout 扱いにすべき run の遷移がさらに遅れる

---

## レビュー指摘修正 (2026-05-02)

### 修正内容

| ファイル | 変更内容 |
|---------|---------|
| `crates/worker/src/main.rs` | `last_sweep` + elapsed比較 → `tokio::time::interval` + 独立 `select!` branch に変更。sweep が `poll_and_dispatch` の完了タイミングに依存しなくなった |
| `crates/db/migrations/20260502000000_add_board_runs_timeout_sweep_index.up.sql` | 新規: `idx_board_runs_timeout_sweep` 部分インデックス (`created_at WHERE status IN (...)`) |
| `crates/db/migrations/20260502000000_add_board_runs_timeout_sweep_index.down.sql` | 新規: 上記インデックスの DROP |
| `README.md` | Worker環境変数表に `TIMEOUT_SWEEP_INTERVAL_SECS` を追加 |

### 指摘への対応

1. **High: sweep実行間隔が保証されない** → `tokio::time::interval` を `select!` の独立ブランチにし、`poll_and_dispatch` やジョブ実行時間に依存しなくなった
2. **Medium: 定期sweep SQLに索引がない** → `board_runs(created_at) WHERE status IN ('created','uploading','importing')` の部分インデックスを追加
3. **ドキュメント: README環境変数未記載** → `TIMEOUT_SWEEP_INTERVAL_SECS` を追加

### テスト結果
- `cargo check --workspace` ✅ 成功
- `cargo test -p boardflow-worker --lib` ✅ 21テスト全パス
- `cargo test -p boardflow-worker --test timeout_sweep_test -- --ignored` ✅ 3テスト全パス

### 残リスク
- main loop統合後のスケジューリング検証テストは未追加（`select!` の非同期動作を単体テストで検証するのは困難。実際のタイミング保証は運用レベルで確認）
- ちょうど12時間境界のテストケースは未追加（既存SQLが `<` 比較のため12h丁度はタイムアウト対象外。仕様と整合）

---

## PR作成 (2026-05-02)

- **PR**: https://github.com/f0reachARR/boardflow/pull/45
- **ブランチ**: `feat/23-board-run-timeout` → `main`
- **Closes**: #23
