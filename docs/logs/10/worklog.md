# Issue #10: 調査: KiCad CLI / InteractiveHtmlBom の Docker 内ヘッドレス利用方法

## Issueまでの経緯

- BoardFlow は GitHub Actions 上の Docker Action で kicad-cli を実行し成果物を生成する設計（docs/spec.md §2.2）
- Docker 内でのヘッドレス実行手順が未ドキュメント化
- samples/ に LightStick KiCad プロジェクトが追加済みで動作確認に利用可能
- 既存 Issue #7（Import Worker）は DRC/ERC レポートの「パース」に言及するが、生成方法は未調査

## ユーザー要望

- KiCad CLI の Docker 内利用方法を調査（Gerber/BOM/PDF/ERC/DRC）
- InteractiveHtmlBom の Docker 内ヘッドレス実行方法を調査
- samples/LightStick で動作確認を含める
- 調査結果を docs/research/kicad-docker-cli.md にまとめる

## 調査結果（初期）

### 判明した事実
- 公式 Docker イメージ: `kicad/kicad:9.0`（Debian 12 Bookworm ベース、amd64/arm64）
- `kicad/kicad:10.0-full` も利用可能（2026-04 時点）
- kicad-cli は headless で動作（GUI 不要）
- DRC: `kicad-cli pcb drc --format json --exit-code-violations`
- ERC: `kicad-cli sch erc --format json --exit-code-violations`（推定）
- Gerber: `kicad-cli pcb export gerbers`
- InteractiveHtmlBom: xvfb + Python venv 内で実行が必要な可能性あり

### Docker イメージ情報
- Dockerfile ソース: https://gitlab.com/kicad/packaging/kicad-cli-docker
- タグポリシー: major.minor は最新パッチを追従、major.minor.patch は固定
- nightly は月次タグあり（例: kicad/kicad:nightly-202604）

## Issue作成内容

- Issue #10: https://github.com/f0reachARR/boardflow/issues/10
- ラベル: research, docker, kicad, documentation
- 区分: 新規作成

## 後続処理タイプの初期仮説

`research_only` — 調査とドキュメント作成のみ。実装は後続Issueで行う。

## 残リスク（初期）

- KiCad 9.0 と 10.0 のどちらを対象にするか（spec.md では 9.0 系を指定）
- InteractiveHtmlBom の KiCad 9 互換性が未確認
- DRC/ERC JSON スキーマが KiCad バージョン間で変わる可能性
- samples/LightStick が ERC/DRC を通るかは未確認

---

## 調査フェーズ（2026-04-30）

### 実施内容

#### 1. KiCad CLI 9.0 公式ドキュメント調査
- https://docs.kicad.org/9.0/en/cli/cli.html からコマンドリファレンス全文を取得
- 対象コマンド: pcb export gerbers, pcb export drill, sch export bom, sch export pdf, pcb export pdf, sch erc, pcb drc, pcb export svg, pcb render

#### 2. Docker Hub kicad/kicad イメージ調査
- kicad/kicad:9.0 = 圧縮 453 MB, 展開 1.67 GB, Debian 12 Bookworm, amd64+arm64
- 含まれるもの: kicad-cli, kicad (GUI), Python 3.11.2, KiCad ライブラリ
- 含まれないもの: pip, xvfb, 日本語フォント
- タグポリシー: 9.0 = 最新パッチ追従, 9.0.8 = 固定

#### 3. InteractiveHtmlBom 調査
- v2.11.1（2026-03-31）が最新、KiCad 9/10 対応
- CLI: `generate_interactive_bom --no-browser --dest-dir <dir> <pcb_file>`
- v2.11.0 で wx dependency が CLI 用に optional になったが、実際には X Display を要求
- Docker 内では `xvfb-run` が必須

#### 4. Docker 動作確認（samples/LightStick）

全コマンドを `kicad/kicad:9.0` Docker イメージで実行確認:

| コマンド | 結果 | 備考 |
|---|---|---|
| `pcb export gerbers` | ✅ | 29 ファイル |
| `pcb export drill` | ✅ | 1 ファイル |
| `sch export bom` | ✅ | CSV 出力 |
| `sch export pdf` | ✅ | 194 KB |
| `pcb export pdf` | ✅ | レイヤー名は `.` 区切り |
| `sch erc --format json` | ✅ | 4 violations |
| `pcb drc --format json` | ✅ | 39 violations |
| `pcb drc --exit-code-violations` | ✅ | exit code 5 |
| `pcb export svg --mode-multi` | ✅ | レイヤーごと |
| `pcb render` | ✅ | 54 KB PNG |
| InteractiveHtmlBom (xvfb-run) | ✅ | 194 KB HTML |

#### 5. DRC/ERC JSON スキーマ確認
- DRC: `$schema: https://schemas.kicad.org/drc.v1.json` — violations, unconnected_items, schematic_parity
- ERC: `$schema: https://schemas.kicad.org/erc.v1.json` — sheets[].violations
- 共通: type, description, severity, items[].{description, pos, uuid}
- 注意: スキーマ URL 自体はリンク切れ（KiCad Issue #23948）

### 成果物

- `docs/research/kicad-docker-cli.md` — 調査結果の完全なドキュメント

### 結論ステータス

`research_only` — 調査とドキュメント作成のみで完了。実装（boardflow-action の Dockerfile、entrypoint スクリプト）は後続 Issue で行う。

### 残リスク（更新）

1. **DRC/ERC JSON スキーマの公式定義が存在しない**: `schemas.kicad.org` のリンクが切れている。実際の出力からスキーマを推測する必要がある
2. **InteractiveHtmlBom の xvfb 依存**: v2.11 で optional になったはずだが実際には必要。将来のバージョンで改善される可能性あり
3. **LightStick の ERC/DRC violation**: テスト用サンプルに4件の ERC エラーと39件の DRC 違反がある。boardflow-action の動作確認時に exit code の扱いに注意
4. **日本語フォント**: Docker イメージに日本語フォントが含まれない。日本語テキストを含む回路図の PDF 出力で文字化けの可能性
5. **KiCad 10 への移行**: 9.0 → 10.0 で DRC/ERC JSON スキーマの破壊的変更の可能性（スキーマ URL のバージョンは v1 のまま）

---

## PR 作成結果（2026-04-30）

### PR 情報

- **PR タイトル**: docs: add KiCad CLI / iBOM Docker headless research
- **PR URL**: https://github.com/f0reachARR/boardflow/pull/11
- **ブランチ**: `research/issue-10-kicad-docker-cli` → `main`
- **コミット**: `9974172` — docs: add KiCad CLI / iBOM Docker headless research (#10)
- **ラベル**: documentation, research
- **コミット対象ファイル**:
  - `docs/research/kicad-docker-cli.md`（新規）
  - `docs/logs/10/worklog.md`（新規）

### review/docs 判定

- **pr_ready**: true（調査 Issue のため実装なし）
- **docs_ready**: true（調査ドキュメント自体が成果物）

### 残リスク（最終）

1. DRC/ERC JSON スキーマ URL がリンク切れ（`schemas.kicad.org`）
2. InteractiveHtmlBom の xvfb 依存は将来バージョンで解消される可能性
3. LightStick に ERC 4件・DRC 39件の violation あり（exit code 注意）
4. Docker イメージに日本語フォントが含まれない
5. KiCad 10 への移行時に DRC/ERC スキーマ破壊的変更の可能性
