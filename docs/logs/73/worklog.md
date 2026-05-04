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

### export_pcb_svg / render_3d APIの変更

`export_pcb_svg` と `render_3d` の side パラメータを `&str` → `PcbSide` enum に変更。`PcbSide::Top` / `PcbSide::Bottom` のみ受け付け、不正な値はコンパイル時に排除される。

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

---

## Review結果 (2026-05-04, 2回目)

### 総評

- 前回レビューで必須としていた4件は、実装・テスト・spec との対応の観点で解消を確認した。
- ただし、修正後の `export_pcb_svg` API に、契約外の `side` 値を受けた際に黙って top 面レイヤーへフォールバックする新しい挙動が入っており、誤った成果物を静かに生成し得る。

### 再確認項目

1. config schema厳格化
	- `BoardflowConfig` / `OutputsConfig` に `#[serde(deny_unknown_fields)]` 追加を確認。
	- `validate_schema_v1` で `outputs.preset` が `default` のみ許可されることを確認。
	- `tests/config_test.rs` に未知フィールド拒否・非対応 preset 拒否の回帰テストを確認。
2. detect fallback
	- `.kicad_pcb` / `.kicad_sch` fallback が「ちょうど1件のみ成功」に修正され、複数候補時に `MultipleKicadPcb` / `MultipleKicadSch` を返すことを確認。
	- `tests/detect_test.rs` に複数候補エラーの回帰テストを確認。
3. CLI オプション
	- `action/lib/kicad.sh` と `crates/kicad/src/cli.rs` を照合し、ERC/DRC、PDF、SVG、Gerber、Drill、BOM、POS、3D render の各引数が一致することを確認。
	- `tests/cli_test.rs` 13件で引数組み立ての回帰を確認。
4. kill_on_drop
	- `exec()` / `exec_erc_drc()` の `Command::new()` に `.kill_on_drop(true)` が追加されていることを確認。
	- Tokio の `kill_on_drop` は strict な reap 保証ではないが、前回指摘していた「drop 既定で継続実行される」問題への修正としては妥当。

### テスト結果

- `mise exec -- cargo test -p boardflow-kicad`: 72 tests passed
- `mise exec -- cargo check --workspace`: passed
- `get_errors` on `crates/kicad`: no errors

### レビュー結果

- `pr_ready: false`

### 必須修正

1. `crates/kicad/src/cli.rs` の `build_pcb_svg_args` が、`side` に `bottom` 以外の任意値を受けると黙って top 面レイヤーへフォールバックする。Issue #73 の作業ログでは `"top" / "bottom" を渡す` 契約として整理されている一方、実装は契約違反入力をエラーにせず誤った成果物生成へ進む。Phase 2 以降で呼び出し側をつないだ際、typo や値変換ミスを検知できないため、`side` を enum 化するか、少なくとも `top|bottom` 以外を `Result` で拒否する必要がある。

### 任意改善

- なし

### テスト不足

- `export_pcb_svg` / `build_pcb_svg_args` に対する不正 `side` 値の失敗テストがない。

### ドキュメント確認

- `docs/spec.md` の Phase 1 対象範囲と、今回確認した4件の修正内容は整合している。
- `docs/external/kicad-docker-cli.md` に記載の ERC/DRC オプションと current 実装は整合している。

### PR/完了結果

- 4件の前回指摘はクローズ可能。
- ただし `export_pcb_svg` の入力検証を入れるまでは PR ready とは判定しない。

### 残リスク

- `kill_on_drop(true)` は timeout 後の継続実行を防ぐ意図には沿うが、Unix 上の zombie cleanup は Tokio の best-effort であり、厳密な後始末保証まではしない。
- KiCad 実バイナリを伴う統合確認は未実施のため、Phase 2 統合時に CLI 実行時差分が出る可能性は残る。

---

## Review結果 (2026-05-04, 3回目)

### 総評

- 前回レビューで唯一の必須修正としていた SVG/3D render の `side` 入力検証は、`PcbSide` enum 導入により API 境界で解消された。
- `crates/kicad/src/cli.rs` では `export_pcb_svg` / `render_3d` / 各 arg builder が `PcbSide` を受け取り、`Top` / `Bottom` 以外の値を型レベルで受け付けない。
- `crates/kicad/tests/cli_test.rs` の top/bottom ケースも `PcbSide::Top` / `PcbSide::Bottom` に更新されており、前回懸念していた「任意文字列が静かに top へフォールバックする」問題は再発していない。

### 再確認項目

1. `PcbSide` enum (`Top`, `Bottom`) が `crates/kicad/src/cli.rs` に追加され、`as_str()` と SVG layer 解決が enum 起点になっていることを確認。
2. `export_pcb_svg` / `render_3d` と `build_pcb_svg_args` / `build_render_3d_args` が `&str` ではなく `PcbSide` を受け取ることを確認。
3. `crates/kicad/src/lib.rs` から `PcbSide` が re-export され、crate 外からも同じ型を利用できることを確認。
4. `crates/kicad/tests/cli_test.rs` で top/bottom の回帰テストが enum ベースに更新されていることを確認。
5. `get_errors` on `crates/kicad`: no errors。

### テスト結果

- ユーザー申告: `cargo test -p boardflow-kicad` 72 tests passed
- ユーザー申告: `cargo check --workspace` passed
- 追加確認: `get_errors` on `crates/kicad` returned no errors

### レビュー結果

- `pr_ready: false`

### 必須修正

1. `docs/logs/73/worklog.md` に旧 API 契約の記述が残っている。Phase 1 修正内容の節で、`export_pcb_svg` の第3引数が `side: &str` であり `"top" / "bottom"` を渡す契約だとまだ記載されているが、現実の実装は `PcbSide` enum へ更新済み。Issue 成果物としての worklog と実装が不一致のままなので、PR 前に記述を最新状態へ直す必要がある。

### 任意改善

- なし

### テスト不足

- 今回の修正観点に限れば追加の不足は見当たらない。enum 化により不正 `side` 値は型レベルで表現不能になっている。

### ドキュメント確認

- `docs/spec.md` の Phase 1 スコープと今回の型安全化は整合している。
- `docs/external/kicad-docker-cli.md` の top/bottom 固定値前提とも矛盾しない。
- ただし `docs/logs/73/worklog.md` の API 説明だけが旧状態のまま残っている。

### PR/完了結果

- 前回レビューの唯一のコード指摘はクローズ可能。
- ただし Issue 作業ログの記述不整合が残るため、このレビューでは PR ready とは判定しない。

### 残リスク

- code と test は整合しているため、残るリスクは主に成果物ドキュメントの誤読による後続 Phase での認識ずれ。

---

## ドキュメント確認 (2026-05-04, Phase 1 docs review)

### 確認対象

- `docs/logs/73/worklog.md`
- `README.md`
- `docs/spec.md`
- `docs/technology.md`
- `crates/kicad/Cargo.toml`
- `docs/external/kicad-docker-cli.md`
- `docs/external/kicad-erc-drc-findings.md`

### 確認結果

- `docs/logs/73/worklog.md` は、Phase 1 実装スコープ（`crates/kicad/` 新規作成、7モジュール、72 tests pass、`PcbSide` enum 導入）と整合している。
- 直前レビュー履歴では `worklog` 自体の不整合を指摘していたが、現時点のファイル本文には `PcbSide` 化後の記述が反映されており、その指摘はクローズ可能。
- `README.md` は現時点では環境構築・実行方法の案内が中心であり、Phase 1 の内部 crate 追加だけを理由にした更新は不要。
- `docs/spec.md` は `.boardflow.yml` schema、project 検出、KiCad CLI 実行方針を既に記述しており、Phase 1 実装内容と矛盾しない。
- `docs/technology.md` は採用技術の上位方針を扱っており、内部用 `crates/kicad/` 追加に伴う更新は不要。
- `docs/external/kicad-docker-cli.md` と `docs/external/kicad-erc-drc-findings.md` の前提は current `crates/kicad/src/cli.rs` / `report.rs` と整合している。

### 必須修正

1. `crates/kicad/Cargo.toml` に `description` がなく、新規 crate の目的が package metadata から判別できない。Issue #73 Phase 1 の成果物として crate 自体の説明を追加する必要がある。

### 任意改善

- なし

### ドキュメント確認結果

- `docs_ready: false`

### PR/完了結果

- 既存ドキュメント（`README.md`, `docs/spec.md`, `docs/technology.md`）に Phase 1 起因の追加更新は不要。
- ただし crate metadata の説明不足が残るため、ドキュメント観点では PR 作成可とは判定しない。

### 残リスク

- `Cargo.toml` metadata が不足したままだと、workspace 外から crate の責務を追う際に意図が伝わりにくい。

---

## Review結果 (2026-05-04, 4回目・最終)

### 総評

- 前回指摘していた `PcbSide` 化の worklog 不整合は解消され、CLI 引数・detect fallback・`kill_on_drop(true)`・crate metadata 追加も実装内容と整合している。
- ただし `.boardflow.yml` schema の厳格化について、spec が要求する「型不一致は検出エラー」と現行実装がまだ一致していない。

### テスト結果

- ユーザー申告: `cargo test -p boardflow-kicad` 72 tests passed
- ユーザー申告: `cargo check --workspace` passed
- 追加確認: `get_errors` on `crates/kicad` returned no errors

### レビュー結果

- `pr_ready: false`

### 必須修正

1. `outputs.preset: null` を現在の実装が許容しており、`docs/spec.md` の schema 要件と矛盾している。spec では「未知フィールド、型不一致、非対応versionは検出エラー」と明記されている一方、`crates/kicad/src/config.rs` の `OutputsConfig` は `preset: Option<String>` のため YAML null を `None` として受理し、`validate_schema_v1` もそれをエラーにしない。さらに `crates/kicad/tests/config_test.rs` には `validate_schema_v1_accepts_no_preset` として `outputs:\n  preset:` を成功ケースにしている。`outputs` 自体の省略は許容してもよいが、`preset` フィールドを明示した上で null を与えるケースは型不一致として reject しないと spec 準拠にならない。

### 任意改善

- なし

### テスト不足

- `outputs.preset: null` を reject する回帰テストがない。spec に合わせるなら、parse もしくは validation のどちらで落とすかを固定し、その失敗テストを追加する必要がある。

### ドキュメント確認

- `docs/logs/73/worklog.md` の `PcbSide` 記述は current 実装と整合している。
- `docs/external/kicad-docker-cli.md` と `crates/kicad/src/cli.rs` の KiCad CLI オプションは整合している。
- 未解消の不整合は `docs/spec.md` の schema 厳格性と `crates/kicad/src/config.rs` / `crates/kicad/tests/config_test.rs` の null 許容のみ。

### PR/完了結果

- 前回までの指摘はクローズできる。
- ただし schema 厳格性の未解消が残るため、この時点では PR ready とは判定しない。

### 残リスク

- `outputs.preset: null` を含む設定を静かに受理すると、MVP の「厳格な schema 検証」を前提にした後続 Phase のエラー設計と齟齬が出る。

---

## Review結果 (2026-05-04, 5回目・Phase 1 最終判定)

### 総評

- 前回の最終ブロッカーだった `outputs.preset: null` 許容は、`OutputsConfig.preset` の `String` 化と `validate_schema_v1` の `"default"` 固定チェックで解消された。
- `crates/kicad/src/config.rs` は `#[serde(deny_unknown_fields)]` を維持しつつ、`outputs` 省略は許容、`preset` の不正値・空値は reject する現在の Phase 1 要件に整合している。
- `crates/kicad/src/cli.rs` の `kill_on_drop(true)`、`PcbSide` enum、bash 同等の引数組み立て、`crates/kicad/Cargo.toml` の metadata 追加も維持されており、既知の必須指摘はクローズ可能。

### 確認結果

- ユーザー申告: `cargo test -p boardflow-kicad` 72 tests passed
- ユーザー申告: `cargo check --workspace` passed
- 追加確認: `mise exec -- cargo test -p boardflow-kicad validate_schema_v1_rejects_null_preset -- --exact` passed
- 追加確認: `get_errors` on `crates/kicad` returned no errors

### レビュー結果

- `pr_ready: true`

### 必須修正

- なし

### 任意改善

- `crates/kicad/src/ibom.rs` は `cli.rs` と異なり timeout / process cleanup 方針をまだ持たないため、Phase 2 以降で実行管理をそろえる余地はある。

### テスト不足

- KiCad / xvfb 実バイナリを使う統合テストは未実施のため、Phase 2 接続時に実行環境差分の確認は引き続き必要。

### ドキュメント確認

- `docs/logs/73/worklog.md`、`docs/spec.md`、`docs/external/kicad-docker-cli.md`、`docs/external/kicad-erc-drc-findings.md` の current 実装との不整合は見当たらない。

### PR/完了結果

- Issue #73 Phase 1 は PR 作成可と判定する。

### 残リスク

- 実行系は単体テスト中心のため、KiCad 実環境での end-to-end 検証は Phase 2 で別途必要。
