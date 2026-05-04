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

---

## Review結果 (2026-05-04)

### 総評

- Phase 1 の crate 追加、workspace 組み込み、hash/report の基礎実装までは進んでいるが、bash 実装との忠実性と spec 準拠に未解決の差分が残る。
- とくに config validation、project file fallback 解決、CLI オプション再現、timeout 挙動に受け入れ条件との不一致があるため、このままの PR 化は不可。

### 再確認テスト結果

- `mise exec -- cargo test -p boardflow-kicad`: 51 tests passed
- `mise exec -- cargo check --workspace`: passed

### レビュー結果

- `pr_ready: false`

### 必須修正

1. `config.rs` が spec の schema v1 を満たしていない。未知フィールド拒否、`outputs.preset` の許可値制約、型検証が未実装で、現状は version しか見ていない。`config_test.rs` でも未知フィールド許容を成功ケースとして固定しており、spec と逆転している。
2. `detect.rs` の `.kicad_pcb` / `.kicad_sch` fallback が「最初に見つかった1件」を返しており、bash 実装の「fallback 時は一意な1件のみ許可」と一致していない。複数候補時に誤った project 解決を引き起こす。
3. `cli.rs` の KiCad 呼び出しが bash 実装を再現していない。ERC/DRC の `--severity-all` と `--exit-code-violations` が欠落し、PDF/SVG/drill/pos/render でも既存オプションが抜けているため、生成物や exit code の意味が変わる。
4. `cli.rs` の timeout 実装は、コメントに反して timeout 時に子プロセス kill を保証していない。`tokio::process::Child` は既定では drop で継続実行されるため、300 秒 timeout の受け入れ条件を満たしたとは言えない。

### 任意改善

1. `detect.rs` に required file exclusion validation 相当の API がなく、`action/lib/detect.sh` の `validate_required_files` 相当ロジックが欠けている。Phase 2 以降で再実装を避けるなら crate に寄せた方がよい。
2. `hash.rs` は `is_excluded` ごとに `GlobSet` を再構築している。ホットパス化するなら compile 済み matcher を保持する API の方が扱いやすい。
3. `ibom.rs` も `KicadCli` と同様の timeout/kill 方針を明示しておくと運用時の一貫性が出る。

### テスト不足

- `cli.rs` のコマンド組み立てテストがなく、既存 bash オプションとの差分を検知できない。
- `detect.rs` の複数 fallback 候補ケース（`.kicad_pcb` 複数、`.kicad_sch` 複数）の失敗テストがない。
- `config.rs` の未知フィールド拒否、`outputs.preset != default` 拒否、型不正拒否のテストがない。
- bash 実装との `tree_hash` 同値性を直接比較する回帰テストがない。
- timeout 時にプロセスが終了することを確認するテストがない。

---

## Phase 1 レビュー指摘修正 (2026-05-04)

### 修正内容

| 指摘 | 修正内容 |
|---|---|
| 1. config schema厳格化 | `BoardflowConfig`, `OutputsConfig` に `#[serde(deny_unknown_fields)]` 追加。`validate_schema_v1` で `outputs.preset` が指定時 `"default"` のみ許可に制限 |
| 2. detect fallback修正 | `resolve_pcb_file`, `resolve_root_schematic` を「一意な1件のみ」に修正。2件以上→`MultipleKicadPcb`/`MultipleKicadSch` エラー追加 |
| 3. CLI オプション修正 | bash実装と同一のオプションに修正（`--severity-all`, `--exit-code-violations`, `--layers`, `--mode-multi`, `--format excellon`, `--excellon-separate-th`, `--format csv`, `--quality basic`）。各コマンドの引数を `build_*_args` pub メソッドに分離 |
| 4. timeout時の明示的kill | `exec()`, `exec_erc_drc()` の `Command::new()` に `.kill_on_drop(true)` を設定。誤解コメント「On timeout, the child future is dropped which kills the process」を削除 |

### export_pcb_svg APIの変更

`export_pcb_svg` の第3引数を `layers: &str` → `side: &str` に変更。`"top"` / `"bottom"` を渡すとbash実装と同じレイヤーセットが適用される。

### 追加/更新テスト

| テストファイル | 変更 |
|---|---|
| `tests/cli_test.rs` | **新規** 13テスト — 全コマンドの引数組み立てをbashと比較 |
| `tests/config_test.rs` | 16テスト（10→16）: 未知フィールド拒否(2件)、preset "custom"/"full" 拒否、version 0 拒否、preset "default" 許可、preset null 許可 |
| `tests/detect_test.rs` | 16テスト（14→16）: 複数 `.kicad_pcb` / `.kicad_sch` 候補時のエラーケース追加 |

### テスト結果

```
cargo test -p boardflow-kicad
72 tests passed (0 failed, 0 ignored)
- cli_test: 13 tests
- config_test: 16 tests
- detect_test: 16 tests
- hash_test: 17 tests
- report_test: 10 tests
```

`cargo check --workspace` もパス。

### 残リスク

- `ibom.rs` は `kill_on_drop` 未適用（改善推奨事項の範囲、Phase 2以降で対応可）
- `hash.rs` の `is_excluded` は毎回 GlobSet 再構築（改善推奨事項の範囲）

### ドキュメント確認

- `docs/spec.md` の `.boardflow.yml` v1 制約と current `config.rs` 実装が不一致。
- `docs/external/kicad-erc-drc-findings.md` と `docs/external/kicad-docker-cli.md` で前提にしている KiCad CLI オプション群が `cli.rs` で再現されていない。
- `worklog.md` 末尾の「drop時にkillされる仕様」という記述は Tokio の既定挙動と一致しないため修正が必要。

### レビュー残リスク

- KiCad 実行環境がなくても通る単体テストだけでは、Phase 2 で command-line 互換性の差分が顕在化する可能性が高い。
- project 検出の誤解決は、誤った board project を build 対象にして Plan/Create payload を壊すリスクがある。
