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
