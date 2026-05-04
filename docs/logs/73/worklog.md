# Issue #73: boardflow-action entrypointをRustバイナリへ移行

## 経緯
- ユーザー要望5: シェルスクリプトベースのentrypointをRustバイナリに移行
- 既存Issue #73 (OPEN) がそのまま要望に合致

## ユーザー要望
- `action/entrypoint.sh` (bash ~530行) を Rust バイナリに移行

## Issue状態
- 既存Issue #73 がOPENで、内容は十分に詳細（Phase 1-4のスコープ、設計方針あり）
- 更新不要、そのまま処理対象とする

## 後続処理タイプ
`implementation_required`

## 残リスク
- Dockerfile内でのRustビルド時間
- Docker imageサイズへの影響

---

## Phase 1 実装 (2026-05-04)

### 実装内容
ブランチ: `feature/73-kicad-crate` (origin/mainから作成)

`crates/kicad/` を新規作成。以下のモジュールを実装:

| モジュール | 内容 |
|---|---|
| `error.rs` | `KicadError` enum (thiserror) — 全エラーを集約 |
| `cli.rs` | `KicadCli` 構造体 — KiCad CLIコマンド10種をラップ（300秒timeout, ERC/DRC exit code 5対応） |
| `report.rs` | `ErcReport` / `DrcReport` — KiCad ERC/DRC JSON出力のserdeパース |
| `detect.rs` | `.boardflow.yml` 探索、`.kicad_pro`/`.kicad_pcb`/`.kicad_sch` 解決 |
| `config.rs` | `.boardflow.yml` YAMLパース、v1バリデーション、excludeマージ |
| `hash.rs` | glob exclude、SHA256計算、tree_hash（ソート済みファイル一覧のハッシュ） |
| `ibom.rs` | `xvfb-run generate_interactive_bom` ラッパー |

### ワークスペース変更
- `Cargo.toml`: `members` に `"crates/kicad"` 追加
- `[workspace.dependencies]` に `serde_yaml`, `globset`, `walkdir`, `tempfile` 追加

### テスト結果
```
cargo test -p boardflow-kicad
51 tests passed (0 failed, 0 ignored)
- config_test: 10 tests (YAML parse/validate, merge_excludes)
- detect_test: 14 tests (find ymls, resolve pro/pcb/sch, project files)
- hash_test: 17 tests (is_excluded, file sha256, tree_hash determinism)
- report_test: 10 tests (ERC/DRC parse, violations filter, has_errors)
```

### 更新ドキュメント
- `docs/logs/73/worklog.md` (本ファイル)

### 残リスク
- CLIテスト（`cli.rs`）はKiCad未インストール環境のため `#[ignore]` 対象（今回テストファイルなし — Phase 2以降でinteg test追加予定）
- `ibom.rs` も同様にxvfb環境が必要なためinteg testはPhase 2以降
- `tokio::process::Child` の `wait_with_output` が ownership を取るため、timeout時の明示的kill は省略（tokio側でdrop時にkillされる仕様）
- `serde_yaml` は deprecated 表示があるが 0.9 系で機能的に問題なし
