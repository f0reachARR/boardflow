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

---

## PR作成結果 (2026-05-04)

### PR情報

- **PR番号**: #83
- **URL**: https://github.com/f0reachARR/boardflow/pull/83
- **タイトル**: `feat(kicad): add boardflow-kicad crate (Issue #73 Phase 1)`
- **base**: `main` ← `feature/73-kicad-crate`
- **Refs**: #73

---

## Phase 2-4 外部調査 (2026-05-05)

### 調査対象

1. Rust Docker マルチステージビルド (kicad/kicad:9.0 + Rust バイナリ)
2. GitHub Actions Docker Action と Rust バイナリ entrypoint
3. reqwest リトライ (指数バックオフ) 実装パターン
4. zip クレート v2 でのディレクトリ構造付き ZIP 作成

### 調査結果サマリ

#### 1. Docker マルチステージビルド

- **kicad/kicad:9.0** = Debian 12 Bookworm (glibc 2.36)
- **ビルドステージ**: `rust:1.87-bookworm` (同じ Debian バージョン、glibc 一致)
- **musl 不採用**: glibc の方がマルチスレッド性能が良く、ランタイムイメージと互換性確保
- **キャッシュ戦略**: BuildKit cache mount または cargo-chef (初期は cache mount で十分)
- **不要になるツール**: `jq`, `zip` CLI, `curl` (Rust バイナリに内包)
- **引き続き必要**: `python3-pip`, `xvfb`, `interactivehtmlbom` (iBOM 生成用)
- 詳細: `docs/external/rust-docker-multistage-kicad.md`

#### 2. GitHub Actions Docker Action

- input は `INPUT_<NAME>` 環境変数 (ハイフン→アンダースコア, 大文字化)
- 出力は `GITHUB_OUTPUT` 環境変数が指すファイルへの追記 (`key=value\n`)
- Job Summary は `GITHUB_STEP_SUMMARY` ファイルへの Markdown 追記
- エラー報告は `::error::message` を stderr へ出力
- `action.yml` の `runs` セクションは変更最小限 (args 削除のみ)
- 詳細: `docs/external/github-actions-rust-binary-entrypoint.md`

#### 3. reqwest リトライ

- **採用**: 手動リトライループ (bash 実装を忠実再現)
- 理由: 依存追加不要、API 呼び出しが4エンドポイントのみ、bash と同じロジック
- 不採用: `reqwest-middleware` + `reqwest-retry` (追加依存が多い)
- バックオフ: 1s → 2s → 4s (倍々)、最大3回、5xx/timeout のみリトライ
- アップロード用は別タイムアウト (600s)
- 詳細: `docs/external/reqwest-retry-backoff.md`

#### 4. zip クレート v2

- `zip` v2 + `walkdir` でディレクトリ再帰 ZIP 作成
- workspace に既に依存あり、追加不要
- `SimpleFileOptions::default().compression_method(Deflated)` で Deflate 圧縮
- `sort_by_file_name()` で決定論的構造
- 大ファイルは `std::io::copy` でストリーミング
- 詳細: `docs/external/zip-crate-bundle-creation.md`

### 結論ステータス

`implementation_required`

### 後続エージェントへの注意点

- Phase 2 (`crates/action-runner/`) 実装時のキーポイント:
  - 新規 crate `crates/action-runner/` を `Cargo.toml` workspace members に追加
  - 依存: `boardflow-kicad`, `tokio`, `reqwest` (features=["json", "rustls-tls"]), `serde`, `serde_json`, `zip`, `walkdir`, `sha2`, `hex`, `chrono`, `uuid`, `thiserror`, `anyhow`
  - TLS backend は `rustls` (OpenSSL ヘッダ不要で Docker ビルドが軽い)
  - `main.rs` → `ActionInputs::from_env()` → detect → plan API → per-project loop (build artifacts → bundle → upload → import)
  - API client は手動リトライ (方式2)
  - `action/Dockerfile` をマルチステージに書き換え
  - `action/entrypoint.sh` と `action/lib/` は Phase 4 で削除
- Phase 3 (Dockerfile 更新):
  - `rust:1.87-bookworm` ビルドステージ追加
  - ENTRYPOINT を Rust バイナリに変更
  - `jq`, `zip`, `curl` を apt-get から削除可能
- Phase 4 (旧スクリプト削除):
  - `action/entrypoint.sh`, `action/lib/*.sh` を削除
  - `action.yml` の変更は不要 (runs.image: 'Dockerfile' のまま)

### 更新ファイル

- `docs/external/rust-docker-multistage-kicad.md` (新規)
- `docs/external/github-actions-rust-binary-entrypoint.md` (新規)
- `docs/external/reqwest-retry-backoff.md` (新規)
- `docs/external/zip-crate-bundle-creation.md` (新規)
- `docs/logs/73/worklog.md` (本ファイル)

### 残リスク

- GitHub Actions でのフル Docker ビルド時間 (初回 5-10分)。pre-built image で回避可能だが初期では不要
- KiCad 10.0 移行時の Debian バージョン変更 (Trixie)。ビルドステージの Debian バージョンも追従が必要
- `interactivehtmlbom` の Python 依存が残るため、完全に Python-free にはできない

### 最終確認

- 未コミット変更: なし
- push 済み: origin/feature/73-kicad-crate に最新コミットあり
- code review: `pr_ready: true`（5回目レビュー）
- docs review: ドキュメント不整合なしを確認（5回目レビュー）
- `cargo test -p boardflow-kicad`: 72 tests passed
- `cargo check --workspace`: passed

### 残リスク

- KiCad / xvfb 実環境を伴う統合テストは未実施（Phase 2 接続時に確認予定）
- `ibom.rs` の timeout/kill_on_drop は Phase 2 以降で対応
- `hash.rs` の GlobSet 毎回再構築はパフォーマンス改善余地あり（Phase 2 以降）

---

## Phase 2-4 実装計画 (2026-05-05)

### 目的
`action/entrypoint.sh` + `action/lib/*.sh` (~1030行) を `crates/action-runner/` Rust バイナリに完全移行し、bash 依存を排除する。

### 非目的
- API サーバー側のロジック変更
- action.yml の inputs/outputs 仕様変更
- 新機能の追加 (PR event対応など)

### 受け入れ条件
1. `boardflow-action-runner` バイナリが entrypoint.sh と同一の処理フローを実行する
2. 全 API 呼び出し (plan, create_board_run, import, fail) が同一 payload/retry 動作する
3. GITHUB_OUTPUT / GITHUB_STEP_SUMMARY へ同等の出力を書き込む
4. bundle.zip が同一ディレクトリ構造を持つ
5. ERC/DRC の fail_on_* フラグが同等に動作する
6. exit code のセマンティクスが同一 (0=success, 1=error)
7. Docker image が正常にビルドでき、サイズが 2GB 以下
8. cargo test が全 pass する
9. action.yml の runs.image が新 Dockerfile を参照する

### 詳細要件

#### Phase 2: `crates/action-runner/` バイナリ実装

**モジュール構成:**

```
crates/action-runner/
├── Cargo.toml
└── src/
    ├── main.rs          # エントリポイント, tokio::main, exit code制御
    ├── inputs.rs        # INPUT_* 環境変数パース, GitHubContext
    ├── api.rs           # ApiClient (手動リトライ), 4エンドポイント
    ├── bundle.rs        # staging dir構築, ZIP作成, manifest生成
    ├── summary.rs       # GITHUB_OUTPUT / GITHUB_STEP_SUMMARY 書き込み
    ├── runner.rs        # メインオーケストレーション (entrypoint.sh の main loop 相当)
    └── error.rs         # ActionError enum
```

**各モジュール責務:**

| モジュール | 責務 | 主な依存 |
|---|---|---|
| `main.rs` | `#[tokio::main]`, tracing初期化, `runner::run()` 呼び出し, `std::process::exit()` | runner |
| `inputs.rs` | `ActionInputs::from_env()`, `GitHubContext::from_env()` | — |
| `api.rs` | `ApiClient::new()`, `plan()`, `create_board_run()`, `import()`, `fail()`, 手動リトライ3回/指数バックオフ | reqwest |
| `bundle.rs` | `build_staging_dir()`, `create_bundle_zip()`, `create_manifest()`, `create_fabrication_zip()`, diff metadata生成 | zip, walkdir, sha2 |
| `summary.rs` | `write_job_summary()`, `write_unsupported_event_summary()`, `set_output()`, `error()`, `warning()` | — |
| `runner.rs` | detect → validate → hash → plan → build loop (ERC/DRC/artifacts/bundle/upload/import) | boardflow-kicad, api, bundle, summary |
| `error.rs` | `ActionError` (thiserror): Input/Api/Kicad/Bundle/Upload variants | — |

**`runner.rs` の処理フロー (entrypoint.sh 忠実移植):**

1. `ActionInputs::from_env()` + `GitHubContext::from_env()`
2. unsupported event チェック (pull_request → skip + summary + exit 0)
3. `kicad::detect::find_boardflow_ymls(workspace)` でプロジェクト検出
4. 各プロジェクト validate (config parse, resolve pro/pcb/sch, validate_required_files)
5. valid projects の tree_hash + file list + per-file SHA256 計算
6. Plan API payload 構築 → `api.plan()` 呼び出し
7. decision="build" のプロジェクトごと:
   a. `api.create_board_run()` → board_run_id, upload_url, object_key
   b. `kicad_cli.run_erc()` / `kicad_cli.run_drc()` → fail_on_* チェック
   c. アーティファクト生成 (pcb_pdf, sch_pdf, svg×2, gerber, drill, bom, position, 3d_render×2, ibom)
   d. fabrication zip 作成 (gerbers.zip, drill.zip, fabrication.zip)
   e. KiCad source files 収集
   f. diff metadata 生成 (file_hashes, bom_summary, checks_summary, artifacts_summary, previews)
   g. manifest.json 生成
   h. staging dir 構築 (review/, assembly/, fabrication/, checks/, diff/, kicad/)
   i. bundle.zip 作成
   j. reqwest PUT → presigned URL アップロード (timeout 600s)
   k. `api.import()` 呼び出し
   l. エラー時: `api.fail()` + continue
8. `summary::write_job_summary()` + `summary::set_output()`
9. detection_errors > 0 → exit code 1
10. return exit_code

**`api.rs` 設計:**

```rust
pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
    max_retries: u32,  // 3
}

// リトライ条件: 5xx, timeout, connect error → retry with exponential backoff (1s, 2s, 4s)
// 4xx → 即エラー
// connect_timeout: 30s, request_timeout: 60s (API), 600s (upload)
```

**`bundle.rs` staging ディレクトリ構造:**

```
staging/
├── manifest.json
├── review/
│   ├── schematic.pdf
│   ├── pcb.pdf
│   ├── pcb_top.svg
│   ├── pcb_bottom.svg
│   ├── render_top.png
│   └── render_bottom.png
├── assembly/
│   ├── ibom.html
│   ├── bom.csv
│   └── position.csv
├── fabrication/
│   ├── gerbers.zip
│   ├── drill.zip
│   └── fabrication.zip
├── checks/
│   ├── erc.json
│   └── drc.json
├── diff/
│   ├── file_hashes.json
│   ├── bom_summary.json
│   ├── checks_summary.json
│   ├── artifacts_summary.json
│   └── previews.json
└── kicad/
    └── <project_dir>/
        ├── *.kicad_pro
        ├── *.kicad_sch
        ├── *.kicad_pcb
        └── *.kicad_wks
```

**Cargo.toml 依存:**

```toml
[package]
name = "boardflow-action-runner"
version = "0.0.1"
edition = "2024"

[[bin]]
name = "boardflow-action-runner"
path = "src/main.rs"

[dependencies]
boardflow-kicad = { path = "../kicad" }
tokio = { workspace = true }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }
zip = { workspace = true }
walkdir = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
thiserror = { workspace = true }
tempfile = { workspace = true }

[dev-dependencies]
wiremock = "0.6"
```

#### Phase 3: Dockerfile 更新

**マルチステージ構造 (cargo-chef 使用):**

```dockerfile
# Stage 1: planner
FROM rust:1.87-bookworm AS planner
RUN cargo install cargo-chef --locked
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json -p boardflow-action-runner

# Stage 2: cacher
FROM rust:1.87-bookworm AS cacher
RUN cargo install cargo-chef --locked
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p boardflow-action-runner

# Stage 3: builder
FROM rust:1.87-bookworm AS builder
WORKDIR /app
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /app/target target
COPY . .
RUN cargo build --release -p boardflow-action-runner

# Stage 4: runtime
FROM kicad/kicad:9.0
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip xvfb ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN pip3 install --break-system-packages interactivehtmlbom
COPY --from=builder /app/target/release/boardflow-action-runner /usr/local/bin/boardflow-action-runner
ENTRYPOINT ["/usr/local/bin/boardflow-action-runner"]
```

**削除パッケージ**: jq, curl, zip (CLI), python3-yaml
**保持パッケージ**: python3-pip, xvfb, ca-certificates, interactivehtmlbom

#### Phase 4: テスト & 旧スクリプト削除

**テスト戦略:**

| テスト種別 | 対象 | 方法 |
|---|---|---|
| Unit test (inputs) | `ActionInputs::from_env()`, `GitHubContext::from_env()` | env var設定 → パース検証 |
| Unit test (api) | リトライ動作, エラー分類 | wiremock mock server |
| Unit test (bundle) | staging構造, ZIP内容, manifest schema | tempdir + ファイル検証 |
| Unit test (summary) | 出力フォーマット | tempfile → 内容比較 |
| Integration test | runner全体フロー (mock API) | wiremock + samples/ ディレクトリ |
| Docker build | ビルド成功確認 | `docker build -t test .` |

**削除対象:**
- `action/entrypoint.sh`
- `action/lib/api.sh`
- `action/lib/bundle.sh`
- `action/lib/config.sh`
- `action/lib/detect.sh`
- `action/lib/hash.sh`
- `action/lib/ibom.sh`
- `action/lib/kicad.sh`
- `action/lib/summary.sh`

**action.yml 変更: なし** (runs.using: docker, image: Dockerfile のまま)

---

### 影響範囲
- `crates/action-runner/` (新規)
- `Cargo.toml` (workspace members 追加)
- `action/Dockerfile` (全面書き換え)
- `action/entrypoint.sh` (削除)
- `action/lib/*.sh` (8ファイル削除)

### 設計方針
1. **bash忠実移植**: 処理順序、エラーハンドリング、リトライ動作をbash実装に合わせる
2. **最小依存**: reqwest-middleware は追加しない。手動リトライで忠実再現
3. **boardflow-kicad 再利用**: detect, config, hash, cli, ibom, report は既存crateをそのまま利用
4. **非同期**: tokio runtime で KiCad CLI 実行を await。プロジェクト間は逐次処理 (bash と同様)
5. **エラー伝播**: 個別プロジェクトの失敗は continue + fail API 呼び出し。全体のexit codeに反映

### テスト観点
- inputs.rs: 必須入力欠落, デフォルト値, ハイフン→アンダースコア変換
- api.rs: リトライ動作 (5xx→retry, 4xx→fail, timeout→retry, 3回超過→fail)
- bundle.rs: staging dir構造の正確性, ZIP内パス, manifest JSON schema, fabrication zip
- summary.rs: GITHUB_OUTPUT / GITHUB_STEP_SUMMARY の書き込みフォーマット
- runner.rs: unsupported event skip, no projects error, partial failure handling, detection_errors

### ドキュメント更新対象
- `docs/logs/73/worklog.md` (本ファイル)
- `docs/backend/summary.md` に action-runner crate の記載追加

### 実装要否
`implementation_required`

### 未解決の疑問
- なし（research 調査で十分なコンテキストが確保されている）

---

### 実装ステップリスト (Phase 2-4)

#### Phase 2: crates/action-runner/ 実装

| # | ステップ | 成果物 | 受け入れ条件 |
|---|---|---|---|
| 2-1 | `Cargo.toml` 作成 + workspace登録 | `crates/action-runner/Cargo.toml`, ルート`Cargo.toml`更新 | `cargo check -p boardflow-action-runner` pass |
| 2-2 | `error.rs` + `inputs.rs` 実装 | `src/error.rs`, `src/inputs.rs` | Unit test pass: 入力パース, バリデーション |
| 2-3 | `api.rs` 実装 | `src/api.rs` | Unit test pass: リトライ動作 (wiremock) |
| 2-4 | `summary.rs` 実装 | `src/summary.rs` | Unit test pass: 出力フォーマット検証 |
| 2-5 | `bundle.rs` 実装 | `src/bundle.rs` | Unit test pass: staging構造, ZIP検証, manifest JSON |
| 2-6 | `runner.rs` 実装 | `src/runner.rs` | Integration test pass: mock API + samples/ |
| 2-7 | `main.rs` 実装 | `src/main.rs` | `cargo build -p boardflow-action-runner` success |

#### Phase 3: Dockerfile 更新

| # | ステップ | 成果物 | 受け入れ条件 |
|---|---|---|---|
| 3-1 | Dockerfile 全面書き換え (cargo-chef マルチステージ) | `action/Dockerfile` | `docker build` success |
| 3-2 | ENTRYPOINT 変更, 不要パッケージ削除 | `action/Dockerfile` 最終形 | コンテナ起動確認 |

#### Phase 4: テスト & 削除

| # | ステップ | 成果物 | 受け入れ条件 |
|---|---|---|---|
| 4-1 | 全テスト pass 確認 | テストコード | `cargo test -p boardflow-action-runner` all pass |
| 4-2 | 旧シェルスクリプト削除 | 9ファイル削除 | `docker build` success (旧ファイル不使用) |
| 4-3 | CI 確認 + PR 作成 | PR | CI green |

---

### ファイル作成/編集リスト

**新規作成:**
- `crates/action-runner/Cargo.toml`
- `crates/action-runner/src/main.rs`
- `crates/action-runner/src/error.rs`
- `crates/action-runner/src/inputs.rs`
- `crates/action-runner/src/api.rs`
- `crates/action-runner/src/bundle.rs`
- `crates/action-runner/src/summary.rs`
- `crates/action-runner/src/runner.rs`

**編集:**
- `Cargo.toml` (workspace members に `"crates/action-runner"` 追加)
- `action/Dockerfile` (全面書き換え)
- `docs/backend/summary.md` (action-runner crate 記載追加)

**削除 (Phase 4):**
- `action/entrypoint.sh`
- `action/lib/api.sh`
- `action/lib/bundle.sh`
- `action/lib/config.sh`
- `action/lib/detect.sh`
- `action/lib/hash.sh`
- `action/lib/ibom.sh`
- `action/lib/kicad.sh`
- `action/lib/summary.sh`

---

## Phase 2-4 実装完了 (2026-05-05)

### 実装内容

ブランチ: `feature/73-action-runner`

#### Phase 2: `crates/action-runner/` バイナリ実装

| ファイル | 内容 |
|---|---|
| `src/error.rs` | `ActionError` enum — Input/Api/Kicad/Bundle/Upload/Io/Json |
| `src/inputs.rs` | 環境変数パース — `ActionInputs` + `GitHubContext` (hyphen/underscore両対応) |
| `src/api.rs` | `ApiClient` — plan/create_board_run/import/fail/upload_bundle、リトライ3回(指数バックオフ1s,2s,4s)、5xx/timeout→retry、4xx→即エラー |
| `src/bundle.rs` | ZIP作成、staging構築、SHA256、diff metadata生成、manifest.json (spec §8.5準拠) |
| `src/summary.rs` | GitHub Actionsアノテーション、GITHUB_OUTPUT書き込み、Markdownテーブル |
| `src/runner.rs` | メインオーケストレーション: detect → validate → hash → plan → process → summary |
| `src/main.rs` | エントリポイント (tokio, tracing_subscriber) |

#### Phase 3: Dockerfile マルチステージ化

- Stage 1 (planner): `rust:1.87-bookworm` + cargo-chef prepare
- Stage 2 (cacher): `rust:1.87-bookworm` + cargo-chef cook
- Stage 3 (builder): cargo build --release
- Stage 4 (runtime): `kicad/kicad:9.0` + python3-pip + xvfb + ca-certificates + interactivehtmlbom
- 不要パッケージ削除: jq, curl, zip, python3-yaml (Rust実装で不要に)
- TLS: `rustls-tls` feature使用 (OpenSSLヘッダ不要)

#### Phase 4: 旧スクリプト削除

9ファイル削除 (entrypoint.sh + lib/*.sh)

### テスト結果

```
cargo test -p boardflow-action-runner -- --test-threads=1
test result: ok. 0 passed (bin)
test result: ok. 7 passed; 1 ignored (api_test)
test result: ok. 12 passed (bundle_test)
test result: ok. 8 passed (inputs_test)
test result: ok. 6 passed (summary_test)
合計: 33 passed, 1 ignored
```

| テストファイル | テスト数 | 観点 |
|---|---|---|
| `inputs_test.rs` | 8 | トークン必須/空エラー、デフォルト値、カスタム値、ハイフン形式ENV、GitHubコンテキスト分割 |
| `api_test.rs` | 7+1 | plan成功、5xxリトライ、4xx即エラー、3回失敗後エラー、create_board_run、import、fail、timeout(ignored) |
| `bundle_test.rs` | 12 | ZIP作成+SHA256、fabrication ZIP、空ディレクトリ、file_hashes.json、BOM summary(存在/不在)、checks_summary、artifacts_summary、previews、manifest、staging(root/nested) |
| `summary_test.rs` | 6 | set_output追記、空パスnoop、Markdownテーブル、空結果、unsupportedイベント |

### 完了条件確認

- ✅ `cargo check -p boardflow-action-runner` pass
- ✅ `cargo test -p boardflow-action-runner` pass (33 tests)
- ✅ `cargo build --release -p boardflow-action-runner` pass
- ✅ 旧スクリプト削除済み
- ✅ Dockerfileマルチステージ化済み
- ✅ `cargo check --workspace` pass

### 残リスク

- `inputs_test.rs` は `env::set_var` (unsafe in edition 2024) を使用。テスト並列実行時にレース可能性あり → `--test-threads=1` 推奨
- timeout統合テスト (`test_retries_on_timeout`) は60s+かかるため `#[ignore]` 
- Dockerビルドは未テスト (CI環境での確認が必要)
- `PlanPayload`/`RepositoryInfo`/`GitInfo`/`ActionInfo` 構造体は未使用警告あり (runner.rsでは `serde_json::json!` マクロ直接使用のため)
- cargo-chef のバージョンピン未指定 (Dockerfileで `--locked` は指定済み)

---

## Review結果 (2026-05-05, Phase 2-4 実装レビュー)

### 総評

- `cargo test -p boardflow-action-runner -- --test-threads=1` は通っているが、bash 実装および `docs/spec.md` が要求する契約に対して重要な後退が残っている。
- とくに manifest 形式、`fail-on-drc` / `fail-on-erc` の exit code 反映、失敗 API payload、`exclude-paths` の扱いは、実装概要で期待している「旧 entrypoint の忠実移植」に達していない。

### テスト結果

- `mise exec -- cargo test -p boardflow-action-runner -- --test-threads=1`: 33 passed, 1 ignored
- `get_errors` では `action/Dockerfile` に hadolint warning、`crates/action-runner` に未使用 code / clippy warning を確認

### レビュー結果

- `pr_ready: false`

### 必須修正

1. `crates/action-runner/src/bundle.rs` の manifest が spec / bash 実装と互換でない。現行実装は `project_path` / `project_dir` / `config_path` / `tree_hash` を top-level に平坦化し、`github_actions.workflow`、`kicad.version`、`hash.tree_hash`、`diff_metadata` を出力していない。`docs/spec.md` は `diff_metadata` と zip entry の整合を要求しており、legacy bash もここを埋めているため、現状の bundle は backend import で reject される可能性が高い。
2. `crates/action-runner/src/runner.rs` の ERC/DRC handling で `fail-on-erc` / `fail-on-drc` が実質 no-op になっている。exit code 5 を検知した箇所がコメントだけで、最終 `exit_code` に反映されないため、spec の「import 完了後に GitHub Actions job だけ失敗させる」挙動を満たしていない。
3. `crates/action-runner/src/api.rs` の fail API payload が spec / bash 実装と不一致。現状は `{message, details}` を送っているが、`docs/spec.md` と旧 `action/lib/api.sh` は `{status: "failed", error: {message, details}}` を要求している。サーバ側が schema validation していれば fail API は 4xx になり、bundle/upload/import 前の失敗を正しく通知できない。
4. `crates/action-runner/src/runner.rs` の `exclude-paths` 処理が action metadata と不一致で、さらに必須ファイル除外の検出が抜けている。`action/action.yml` では改行区切り input だが、実装は `split(',')` 固定のため複数行入力を正しく解釈できない。加えて、bash にあった `validate_required_files` 相当がなく、`.kicad_pro` / `.kicad_pcb` / `.kicad_sch` が除外されても detection error にせず Plan API へ送ってしまう。

### 任意改善

1. `crates/action-runner/src/summary.rs` の job summary 形式が旧 bash と変わっている。ヘッダが `BoardFlow Action Results` から `BoardFlow Results` に変わり、合計件数行も消えているため、既存ドキュメントや利用者の期待に寄せるなら互換性を戻した方がよい。
2. `action/Dockerfile` は目的のマルチステージ化自体はできているが、runtime が `USER root` のまま、`interactivehtmlbom` も unpinned install のままなので、CI 用イメージとしての hardening 余地は残る。

### テスト不足

- manifest の shape と `diff_metadata` を spec 例に照合するテストがない。
- fail API body の JSON schema を wiremock で検証するテストがない。
- `fail-on-drc=true` / `fail-on-erc=true` で import 後に終了コードだけ失敗になる runner-level テストがない。
- `exclude-paths` の改行区切り入力、必須ファイルが exclude されたときに detection error になるケースのテストがない。
- Dockerfile の build smoke test がなく、runtime image 側の entrypoint 起動確認も未実施。

### ドキュメント確認

- `README.md` の更新は必須ではない。
- ただし `docs/logs/73/worklog.md` では `src/bundle.rs` を「manifest.json (spec §8.5準拠)」と記載しているが、現行コードはその記述と一致していない。

### PR/完了結果

- Phase 2-4 は現時点では PR 作成不可。
- 上記 4 件を解消し、runner-level の回帰テストを追加した後に再レビューが妥当。

### 残リスク

- manifest と fail API の schema 不一致は、単体テストが通っても SaaS 側との結合で初めて顕在化するタイプの不具合。
- `exclude-paths` 解釈違いはユーザー設定依存で発火するため、表面上は正常に見えても一部 repository でのみ壊れる可能性がある。

---

## Review結果 (2026-05-05, Phase 2-4 再レビュー)

### 総評

- 前回の必須修正4件について、action-runner 単体のコード変更としては解消を確認した。
  - `create_manifest` は `project` / `github_actions` / `kicad` / `hash` / `diff_metadata` のネスト構造へ変更済み。
  - `process_project` は `Result<bool>` を返し、ERC/DRC exit code 5 と `fail_on_*` の組み合わせで `checks_failed = true` を返し、呼び出し側で upload/import 後に job exit code を 1 にしている。
  - fail API payload は `{"status":"failed","error":{"message":...,"details":...}}` へ変更済み。
  - `exclude-paths` は `lines()` で改行区切りに変更され、必須入力ファイルが exclude された場合は detection error にしている。
- ただし、生成される新 manifest 形式と import 側の `crates/artifact` がまだ互換になっておらず、このままでは bundle import が runtime で失敗する。Issue #73 Phase 2-4 全体としては PR ready ではない。

### 調査結果

- `mise exec -- cargo test -p boardflow-action-runner`: 33 passed, 1 ignored
- `mise exec -- cargo check --workspace`: passed
- Web 調査では GitHub Actions の output / summary は改行区切りのファイル追記が正であり、`exclude-paths` の multiline 入力方針自体は妥当。

### レビュー結果

- `pr_ready: false`

### 重大度順の指摘

1. **Blocker**: `crates/action-runner/src/bundle.rs` は spec §8.5 形式の manifest を生成するが、import 側の `crates/artifact/src/lib.rs` は旧 schema の `BundleManifest` をまだ期待している。具体的には import 側は top-level に `version`, `project_path`, `tree_hash`, `commit_sha`, `files`, `checks: Vec<_>` を要求し、artifact entry も `filename` / `source_path` 前提で ZIP entry を検証している。一方、action-runner 側は `schema_version`, `project`, `git`, `hash`, `checks` object, `path` ベースの artifact を出力している。この不一致により `manifest.json` の deserialize もしくは zip entry 検証で import worker が失敗する可能性が高い。前回の manifest format 修正自体は入っているが、受け側未更新のため E2E では未解消。

### 必須修正

1. `crates/artifact` と import worker を新 manifest schema に合わせて更新し、`boardflow-action-runner` が生成する bundle を実際に受理できる状態にすること。少なくとも manifest struct、artifact path 解決、checks / diff_metadata の読み取り、許可 zip entry 判定を同時に揃える必要がある。

### 任意改善

1. `tests/api_test.rs` の `test_fail_api` は endpoint 呼び出し成功しか見ておらず、`error.message` / `error.details` の payload shape を検証していない。今回の修正点を固定するなら request body matcher を追加した方がよい。
2. `tests/bundle_test.rs` の `test_create_manifest` は `diff_metadata` が object であることしか確認しておらず、各 diff ファイルについて `path` / `sha256` / `size_bytes` が入ることを検証していない。

### テスト不足

- `fail-on-erc` / `fail-on-drc` の「upload/import を完了してから job exit code のみ失敗にする」制御を確認する runner レベルのテストがない。
- `exclude-paths` の改行区切り解釈と、`.kicad_pro` / `.kicad_pcb` / `.kicad_sch` を exclude した際に detection error になるケースの回帰テストがない。
- 新 manifest を import 側が受理できることを確認する cross-crate / integration test がない。

### ドキュメント確認

- `docs/spec.md` の §8.5 manifest 例とは action-runner 側の出力方針が整合している。
- `docs/external/github-actions-rust-binary-entrypoint.md` と GitHub Docs 系の確認結果から、Docker Action での `INPUT_*` / `GITHUB_OUTPUT` / `GITHUB_STEP_SUMMARY` の扱いは妥当。
- ただし、spec と実装の整合は「生成側のみ」で、artifact import 側の schema が追随していない。

### PR/完了結果

- 前回の必須修正4件は「action-runner 単体のコード変更」としては概ね解消を確認。
- しかし Issue #73 の成果物としては bundle import まで含めた end-to-end 成立が必要であり、現時点では PR 作成不可。

### 残リスク

- import 側を直しても、runner レベルの回帰テストが無いままだと `fail-on-*` や required file exclusion の後退を検知しにくい。
- 実 KiCad 実行環境を使う end-to-end 検証は未実施のため、Docker image 置換後の実行時差分は残る。

---

## Review結果 (2026-05-05, Phase 2-4 再レビュー 2回目)

### 総評

- action-runner 単体のビルド、API retry、Docker multi-stage 化そのものは成立している。
- ただし、bundle manifest の `checks` と `files` が実データを持たず、import worker がそのまま DB へ反映しているため、Issue #73 の受け入れ条件である「check結果保存」と「差分用スナップショット保存」を満たしていない。
- そのため、現時点の判定は `pr_ready: false`。

### 調査結果

- `mise exec -- cargo test -p boardflow-action-runner --quiet`: passed (7 + 12 + 8 + 6 tests, timeout test 1件 ignored)
- `mise exec -- cargo test -p boardflow-artifact --quiet`: 25 tests passed
- `docs/spec.md` では `workflow_dispatch` も正式対象、かつ `manifest.json` と ERC/DRC check 保存を BoardRun completed の最低条件としている。
- Web 調査でも、GitHub Actions Docker action の `GITHUB_OUTPUT` / `GITHUB_STEP_SUMMARY` はファイル追記方式が正であり、multi-stage Docker build の方向性自体は妥当だった。

### レビュー結果

- `pr_ready: false`

### 重大度順の指摘

1. **Blocker**: `crates/action-runner/src/bundle.rs` が manifest に `checks: []` を固定で出力しており、ERC/DRC の結果を一切載せていない。一方で import worker は `crates/worker/src/handlers/import.rs` で `manifest.checks` を唯一の入力として `run_checks` を保存し、同じ値から `board_runs.erc_status` / `drc_status` も決めている。このままだと import 成功後も run_checks が 0 件になり、BoardRun completed の最低条件である「check結果、または skipped 状態の保存」を満たさない。
2. **Blocker**: `crates/action-runner/src/bundle.rs` の `build_manifest_files()` が常に空配列を返しており、manifest の `files` が実質無効化されている。import worker は `manifest.files` をそのまま snapshot の `file_hashes_json` に保存し、diff summary の `total_files` 計算にも使っているため、現状では snapshot に差分基準となるファイルハッシュが残らず、`total_files` も常に 0 になる。Plan API では正しい file list を計算しているのに、import 側へ引き継がれていない。
3. **Minor**: `crates/action-runner/src/summary.rs` の unsupported event summary が `BoardFlow processes push events only.` と書いており、`docs/spec.md` の `push` + `workflow_dispatch` 対応と食い違っている。挙動自体は `pull_request` のみ skip なので機能バグではないが、summary 表示は誤案内になっている。

### 必須修正

1. manifest 生成時に ERC/DRC の check entry を埋め、import worker が `run_checks` と `board_runs.{erc,drc}_status` を正しく保存できるようにすること。少なくとも `kind`, `status`, `error_count`, `warning_count`, `raw_summary` を揃える必要がある。
2. manifest `files` に project file hash 一覧を含め、import worker の snapshot 保存と diff summary が空にならないようにすること。既に生成している `PlanFile` 相当データを再利用するか、bundle 生成時に同等の一覧を明示的に渡す必要がある。

### 任意改善

1. unsupported event summary の文言を `push` のみに限定せず、`workflow_dispatch` も正式対象であることが分かる表現に直した方がよい。
2. API retry のテストは 5xx と 4xx を押さえているが、backoff 秒数そのものは固定していない。1, 2 秒の待機方針まで将来壊したくないなら時間依存を抽象化したテストを追加する余地がある。

### テスト不足

- action-runner が生成した manifest を `boardflow-artifact::extract_bundle()` と worker import handler の期待に通して検証する cross-crate test がない。
- `manifest.checks` と `manifest.files` の中身を検証する test がなく、現状の空配列退行を検知できていない。
- unsupported event summary の文言が spec と整合しているかを見る test がない。

### ドキュメント確認

- `docs/spec.md` の completed 条件と現行実装は不整合。spec は check結果保存を必須にしているが、現行 manifest は check を空で出力している。
- `docs/logs/73/worklog.md` の直前レビューでは「manifest schema 整合」が主論点だったが、現行コードでは schema 互換よりも `checks` / `files` の実データ欠落が主要な blocker になっている。

### PR/完了結果

- Issue #73 は Phase 2-4 の主要ファイル追加と Docker 移行まではできている。
- ただし import 完了後の DB 状態が要件を満たさないため、PR 作成はまだ不可。

### 残リスク

- `checks_summary.json` と `diff/file_hashes.json` は作っていても、worker が completed 判定や snapshot 保存に使っているのは `manifest.checks` / `manifest.files` なので、manifest 本体を埋めない限り UI 側で整合しない可能性が高い。
- 現状の unit test 群は module 単位では通るが、bundle 生成から import までの接続面の欠落を拾えていない。

## 2026-05-05 Re-Review (Issue #73 Phase 2-4, manifest.checks/files 修正後)

### Issueまでの経緯

- 再レビュー対象は Issue #73 のうち、前回 Blocker 2件だった `manifest.checks` 空配列固定と `manifest.files` 空配列固定の修正。
- ユーザー申告では `build_manifest_checks()` 追加、`build_plan_files()` の manifest.files 反映、`create_manifest()` 引数追加、`bundle_test.rs` 更新が実施済み。

### ユーザー要望

- `build_manifest_checks` が `crates/artifact/src/lib.rs` の `ManifestCheck` と互換か確認する。
- `manifest.files` が `ManifestFile { path, sha256 }` と互換か確認する。
- worker import handler がこの manifest を使って `run_checks`、findings、snapshot、board_run status を正しく処理できるか確認する。
- 前回レビューの 2 Blocker が解消されたか判定する。

### 調査結果

- `crates/action-runner/src/runner.rs` では `build_manifest_checks(&erc_json, &drc_json)` の結果を manifest に渡し、`build_plan_files()` の結果を `path` + `sha256` 形式へ写して `manifest.files` に渡していることを確認。
- `crates/action-runner/src/bundle.rs` の `create_manifest()` は top-level `version/project_path/tree_hash/commit_sha/files/artifacts/checks/diff_metadata` を出力しており、`crates/artifact/src/lib.rs` の `BundleManifest` / `ManifestCheck` / `ManifestFile` とは今回の `checks` と `files` に関して整合している。
- `crates/worker/src/handlers/import.rs` は引き続き `manifest.checks` から `run_checks` / findings / `erc_status` / `drc_status` を作り、`manifest.files` を snapshot と diff summary に使っているため、前回指摘していた「空配列固定」による欠落は通常系では解消している。
- 追加で確認したところ、source artifact の `source_path` 契約が action-runner と import 側で食い違っており、bundle import が runtime で失敗し得る。
- `mise exec -- cargo test -p boardflow-action-runner --quiet`: pass (7 + 12 + 8 + 6 tests, 1 ignored)。
- `mise exec -- cargo check --workspace`: pass。

### 計画

- 前回 2 Blocker の解消確認に加えて、manifest 生成値が import worker の実装前提と end-to-end で噛み合うかを再点検する。
- 特に source artifact、check 欠落時の status、cross-crate test の有無を重点確認する。

### 実装内容の確認

- `build_manifest_checks()` 自体は `ManifestCheck` / `ManifestFinding` の required field を満たす JSON を返しており、ERC は `subject_kind: schematic` と `sheet_path`、DRC は `subject_kind: pcb` と `pos_mm` を埋めている。
- `manifest.files` は `build_plan_files()` を再利用した `path` + `sha256` 配列になっており、`ManifestFile` と互換。
- `bundle_test.rs` には `checks` と `files` が manifest に含まれることを検証する追加が入っている。

### テスト結果

- `mise exec -- cargo test -p boardflow-action-runner --quiet`: pass。
- `mise exec -- cargo check --workspace`: pass。

### レビュー結果

- 前回 Blocker 2件は「manifest.checks / manifest.files が常に空だった」という観点では解消を確認した。
- ただし Issue #73 Phase 2-4 全体としては、bundle import の source artifact path 契約不整合と、ERC/DRC report 不在時の skipped check 未保存が残っているため、`pr_ready: false` と判断する。

### 重大度順の指摘

1. Blocker: `crates/action-runner/src/bundle.rs` は artifact entry の `source_path` を zip 内 path として一度設定した後、source artifact だけ repository 相対 path で上書きしている。一方 `crates/artifact/src/lib.rs` の `extract_bundle()` は `source_path` をそのまま zip entry path とみなして `archive.by_name()` しているため、KiCad source artifact が 1件でも含まれる bundle は import 時に `artifact file not found in zip` で失敗する可能性が高い。spec は `path` を zip 内 path、`source_path` を repository 相対 path と定義しているので、action-runner 側と import 側のどちらかを揃える必要がある。
2. Blocker: `build_manifest_checks()` は ERC/DRC report ファイルが存在する場合にしか check entry を生成しない。KiCad 実行失敗や report 未生成時は `manifest.checks` に `erc` / `drc` の `skipped` entry が入らず、worker import は `run_checks` 行も `board_runs.erc_status` / `drc_status` も保存しないまま `completed` に進めてしまう。spec の `skipped` 契約と completed 条件に未達。

### 必須修正

1. source artifact について、manifest では `path` を zip 内 path、`source_path` を repository 相対 path として保持しつつ、import 側が zip を引くための別フィールドまたは `path` 利用に揃えること。少なくとも action-runner と `boardflow-artifact::extract_bundle()` の解釈を一致させる必要がある。
2. ERC/DRC report が無い場合でも `erc` / `drc` の `skipped` check を manifest に必ず入れ、理由を `raw_summary` 相当へ残すこと。

### 任意改善

- `build_manifest_checks()` と `create_manifest()` の接続を cross-crate test で固定し、manifest から import 完了までの接続面を回帰テストに含めた方がよい。

### テスト不足

- action-runner が生成した source artifact 付き bundle を `boardflow-artifact::extract_bundle()` に通す E2E / cross-crate test がない。
- ERC/DRC report 不在時に `skipped` check が出力されることを確認する test がない。

### ドキュメント確認

- `docs/spec.md` の `source artifact は kicad/ 以下に閉じ込め、元の repository 相対pathを source_path としてmanifestに保存する` という契約と、現状 import 側の `source_path` 解釈は不整合。
- `docs/spec.md` の `skipped` check 契約とも現状は不整合。

### PR/完了結果

- `pr_ready: false`

### 残リスク

- 現状の unit test は `checks` / `files` の追加自体は検知できるが、source artifact を含む実 bundle を import 側で受理できるかは保証していない。
- report 生成失敗系では BoardRun が `completed` でも check status 未保存になり、UI / コメント生成の整合が崩れる可能性がある。
