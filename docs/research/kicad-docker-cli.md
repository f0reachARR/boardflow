# KiCad CLI / InteractiveHtmlBom の Docker 内ヘッドレス利用調査

Issue: #10
調査日: 2026-04-30
対象: KiCad 9.0 系 (kicad-cli 9.0.8)

## 1. Docker イメージ情報

### kicad/kicad:9.0

| 項目 | 値 |
|---|---|
| ベースOS | Debian 12 Bookworm |
| アーキテクチャ | amd64, arm64 |
| 圧縮サイズ | 約 453 MB |
| 展開後サイズ | 約 1.67 GB |
| KiCad バージョン | 9.0.8 (2026-03-19 ビルド) |
| Python | 3.11.2 |
| pip | なし（要インストール） |
| wxWidgets | 3.2.2 |
| OpenCASCADE | 7.6.3 |
| Boost | 1.74.0 |
| FreeType | 2.12.1 |
| Dockerfile ソース | https://gitlab.com/kicad/packaging/kicad-cli-docker |

### タグポリシー

| タグ形式 | 説明 |
|---|---|
| `9.0.8` | 特定パッチバージョンに固定 |
| `9.0` | 最新パッチを追従（現在 9.0.8） |
| `nightly` | master ブランチの毎晩ビルド |
| `nightly-YYYYMM` | 月次固定 |

### 含まれるツール

- `kicad-cli`: CLI ツール（Gerber/BOM/PDF/DRC/ERC/3D レンダリング等）
- `kicad`: GUI アプリケーション（headless 環境では使わない）
- KiCad ライブラリ: footprints, symbols, schemas, templates
- Python 3.11.2（pip は含まれない）

### 10.0 との比較

| 項目 | 9.0 | 10.0-full |
|---|---|---|
| 圧縮サイズ | 453 MB | 1.34 GB |
| 安定性 | stable release | stable release |
| 対象 | BoardFlow MVP | 将来的な移行先 |

**判断**: spec.md に従い KiCad 9.0 系を使用する。10.0 は DRC/ERC JSON スキーマの互換性に注意が必要。

---

## 2. kicad-cli コマンドリファレンス

### 2.1 Gerber 出力: `kicad-cli pcb export gerbers`

**Docker 実行例**:
```bash
docker run --rm \
  -v $(pwd)/project:/workspace \
  -v $(pwd)/output:/output \
  kicad/kicad:9.0 \
  kicad-cli pcb export gerbers \
    --output /output/gerbers/ \
    /workspace/Board.kicad_pcb
```

**主要オプション**:
| オプション | 説明 |
|---|---|
| `--output <dir>` | 出力ディレクトリ |
| `--layers <list>` | カンマ区切りレイヤーリスト（省略時: 全レイヤー） |
| `--no-x2` | 拡張 X2 フォーマットを使わない |
| `--no-netlist` | ネットリスト属性を含めない |
| `--use-drill-file-origin` | ドリルファイル原点を使用 |
| `--precision <5|6>` | 精度（デフォルト: 6） |
| `--no-protel-ext` | .gbr 拡張子を使用（Protel 拡張子の代わり） |
| `--board-plot-params` | ボードファイルに設定済みの Gerber 設定を使用 |
| `--subtract-soldermask` | はんだマスクのない領域からシルクを除去 |

**出力構造**（LightStick での実測結果）:
```
gerbers/
├── LightStick-F_Cu.gtl
├── LightStick-B_Cu.gbl
├── LightStick-F_Paste.gtp
├── LightStick-B_Paste.gbp
├── LightStick-F_Silkscreen.gto
├── LightStick-B_Silkscreen.gbo
├── LightStick-F_Mask.gts
├── LightStick-B_Mask.gbs
├── LightStick-Edge_Cuts.gm1
├── LightStick-F_Fab.gbr
├── LightStick-B_Fab.gbr
├── LightStick-F_Courtyard.gbr
├── LightStick-B_Courtyard.gbr
└── ... (全 29 ファイル)
```

**ドリルファイルも別途必要**:
```bash
kicad-cli pcb export drill \
  --output /output/drill/ \
  --format excellon \
  --excellon-separate-th \
  /workspace/Board.kicad_pcb
```

**動作確認結果**: ✅ 成功

### 2.2 BOM 出力: `kicad-cli sch export bom`

**Docker 実行例**:
```bash
docker run --rm \
  -v $(pwd)/project:/workspace \
  -v $(pwd)/output:/output \
  kicad/kicad:9.0 \
  kicad-cli sch export bom \
    --output /output/bom.csv \
    /workspace/Board.kicad_sch
```

**主要オプション**:
| オプション | 説明 |
|---|---|
| `--output <file>` | 出力ファイルパス（デフォルト: .csv） |
| `--fields <list>` | エクスポートするフィールドリスト（デフォルト: "Reference,Value,Footprint,${QUANTITY},${DNP}"） |
| `--labels <list>` | ラベル（デフォルト: "Refs,Value,Footprint,Qty,DNP"） |
| `--group-by <fields>` | グルーピングフィールド |
| `--sort-field <field>` | ソートフィールド（デフォルト: "Reference"） |
| `--exclude-dnp` | DNP コンポーネントを除外 |
| `--field-delimiter <delim>` | フィールド区切り文字（デフォルト: ","） |
| `--string-delimiter <delim>` | 文字列囲み文字 |

**出力例**（LightStick での実測結果）:
```csv
"Refs","Value","Footprint","Qty","DNP"
"D1","LED_RGBW","parts:XL-5050RGBW","1",""
"J1","+BATT","TestPoint:TestPoint_Pad_D2.5mm","1",""
"R1","5.1k","Resistor_SMD:R_0603_1608Metric_Pad0.98x0.95mm_HandSolder","1",""
...
```

**レガシー BOM** (XML 形式):
```bash
kicad-cli sch export python-bom --output /output/bom.xml /workspace/Board.kicad_sch
```

**動作確認結果**: ✅ 成功

### 2.3 回路図 PDF 出力: `kicad-cli sch export pdf`

**Docker 実行例**:
```bash
docker run --rm \
  -v $(pwd)/project:/workspace \
  -v $(pwd)/output:/output \
  kicad/kicad:9.0 \
  kicad-cli sch export pdf \
    --output /output/schematic.pdf \
    /workspace/Board.kicad_sch
```

**主要オプション**:
| オプション | 説明 |
|---|---|
| `--output <file>` | 出力ファイルパス |
| `--theme <name>` | テーマ名 |
| `--black-and-white` | 白黒出力 |
| `--no-background-color` | 背景色なし |
| `--pages <list>` | 出力ページ指定（カンマ区切り） |
| `--exclude-drawing-sheet` | 図枠を除外 |
| `--default-font <name>` | デフォルトフォント（デフォルト: "KiCad Font"） |
| `--exclude-pdf-property-popups` | PDF プロパティポップアップを除外 |
| `--exclude-pdf-metadata` | PDF メタデータを除外 |

**フォントに関する注意**:
- Docker イメージには KiCad Font が含まれている
- 日本語フォントは含まれていない（カスタムフォント使用時は Dockerfile でインストール要）
- `--default-font` で代替フォントを指定可能

**動作確認結果**: ✅ 成功（194 KB）

### 2.4 PCB PDF 出力: `kicad-cli pcb export pdf`

**Docker 実行例**:
```bash
docker run --rm \
  -v $(pwd)/project:/workspace \
  -v $(pwd)/output:/output \
  kicad/kicad:9.0 \
  kicad-cli pcb export pdf \
    --layers F.Cu,B.Cu,F.Silkscreen,B.Silkscreen,Edge.Cuts \
    --output /output/pcb.pdf \
    /workspace/Board.kicad_pcb
```

**注意**: レイヤー名は `Edge.Cuts`（ドット区切り）であり、`Edge_Cuts`（アンダースコア）ではない。

**動作確認結果**: ✅ 成功

### 2.5 ERC: `kicad-cli sch erc`

**Docker 実行例**:
```bash
docker run --rm \
  -v $(pwd)/project:/workspace \
  -v $(pwd)/output:/output \
  kicad/kicad:9.0 \
  kicad-cli sch erc \
    --format json \
    --severity-all \
    --exit-code-violations \
    --output /output/erc.json \
    /workspace/Board.kicad_sch
```

**オプション**:
| オプション | 説明 |
|---|---|
| `--format <report\|json>` | レポート形式（デフォルト: report） |
| `--severity-all` | 全 severity を含む |
| `--severity-error` | error のみ |
| `--severity-warning` | warning のみ |
| `--severity-exclusions` | 除外された violation も含む |
| `--exit-code-violations` | 違反があれば exit code 5 を返す |
| `--units <mm\|in\|mils>` | 単位（デフォルト: mm） |

**Exit code**:
| Exit code | 意味 |
|---|---|
| 0 | 正常終了（違反なし、または `--exit-code-violations` なし） |
| 5 | 違反あり（`--exit-code-violations` 指定時のみ） |

**JSON 出力スキーマ** (`$schema: https://schemas.kicad.org/erc.v1.json`):
```json
{
  "$schema": "https://schemas.kicad.org/erc.v1.json",
  "coordinate_units": "mm",
  "date": "2026-04-30T13:45:40+0000",
  "included_severities": ["error", "warning", "exclusion"],
  "kicad_version": "9.0.8",
  "source": "Board.kicad_sch",
  "sheets": [
    {
      "path": "/",
      "uuid_path": "/xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
      "violations": [
        {
          "type": "power_pin_not_driven",
          "description": "Input Power pin not driven by any Output Power pins",
          "severity": "error",
          "items": [
            {
              "description": "Symbol #PWR019 Pin 1 [Power input, Line]",
              "pos": { "x": 0.5715, "y": 0.2667 },
              "uuid": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
            }
          ]
        }
      ]
    }
  ]
}
```

**動作確認結果**: ✅ 成功（LightStick: 4 violations）

### 2.6 DRC: `kicad-cli pcb drc`

**Docker 実行例**:
```bash
docker run --rm \
  -v $(pwd)/project:/workspace \
  -v $(pwd)/output:/output \
  kicad/kicad:9.0 \
  kicad-cli pcb drc \
    --format json \
    --severity-all \
    --exit-code-violations \
    --output /output/drc.json \
    /workspace/Board.kicad_pcb
```

**オプション**:
| オプション | 説明 |
|---|---|
| `--format <report\|json>` | レポート形式（デフォルト: report） |
| `--all-track-errors` | 全トラックエラーを報告 |
| `--schematic-parity` | PCB とスケマティックの整合性チェック |
| `--severity-all` | 全 severity を含む |
| `--exit-code-violations` | 違反があれば exit code 5 を返す |
| `--units <mm\|in\|mils>` | 単位（デフォルト: mm） |

**Exit code**: ERC と同じ（0 = 正常、5 = 違反あり）

**JSON 出力スキーマ** (`$schema: https://schemas.kicad.org/drc.v1.json`):
```json
{
  "$schema": "https://schemas.kicad.org/drc.v1.json",
  "coordinate_units": "mm",
  "date": "2026-04-30T13:45:52+0000",
  "included_severities": ["error", "warning", "exclusion"],
  "kicad_version": "9.0.8",
  "source": "Board.kicad_pcb",
  "violations": [
    {
      "type": "items_not_allowed",
      "description": "Items not allowed (keepout area)",
      "severity": "error",
      "items": [
        {
          "description": "Footprint SW2",
          "pos": { "x": 194.0, "y": 119.25 },
          "uuid": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
        }
      ]
    }
  ],
  "unconnected_items": [],
  "schematic_parity": []
}
```

**DRC violation types**（LightStick で確認されたもの）:
- `items_not_allowed` — キープアウト領域への侵入
- `courtyards_overlap` — コートヤードの重なり
- `silk_edge_clearance` — シルクスクリーンのボードエッジクリアランス

**動作確認結果**: ✅ 成功（LightStick: 39 violations, 0 unconnected）

### 2.7 PCB SVG 出力: `kicad-cli pcb export svg`

```bash
kicad-cli pcb export svg \
  --layers F.Cu,B.Cu,F.Silkscreen,Edge.Cuts \
  --mode-multi \
  --output /output/svg/ \
  /workspace/Board.kicad_pcb
```

**注意**: `--mode-multi` を明示的に指定すること。KiCad 9.0 ではデフォルト動作が deprecated であり、将来 `--mode-multi` がデフォルトになる。

**動作確認結果**: ✅ 成功

### 2.8 3D レンダリング: `kicad-cli pcb render`

```bash
kicad-cli pcb render \
  --output /output/render-top.png \
  --side top \
  --quality basic \
  /workspace/Board.kicad_pcb
```

**オプション**:
- `--side`: top, bottom, left, right, front, back
- `--quality`: basic (デフォルト), high, user
- `--width` / `--height`: 解像度（デフォルト: 1600x900）
- `--background`: default, transparent, opaque

**動作確認結果**: ✅ 成功（54 KB PNG）

---

## 3. InteractiveHtmlBom (iBOM)

### 概要

| 項目 | 値 |
|---|---|
| バージョン | v2.11.1（2026-03-31 リリース） |
| KiCad 9 互換性 | ✅ v2.10.0 以降で padstack サポート |
| KiCad 10 互換性 | ✅ v2.11.0 以降 |
| インストール方法 | `pip install interactivehtmlbom` |
| ライセンス | MIT |
| wxPython 要否 | v2.11.0 以降 CLI 使用時は optional（ただし Docker 内では xvfb 要） |

### Docker 内実行方法

**重要: xvfb が必要**

v2.11.0 で wx dependency が optional になったが、実際の Docker 実行では X Display が要求される。`xvfb-run` で仮想ディスプレイを提供する必要がある。

```bash
docker run --rm --user root \
  -v $(pwd)/project:/workspace \
  -v $(pwd)/output:/output \
  kicad/kicad:9.0 \
  bash -c "
    apt-get update -qq > /dev/null 2>&1 && \
    apt-get install -y -qq python3-pip xvfb > /dev/null 2>&1 && \
    pip install --break-system-packages interactivehtmlbom && \
    xvfb-run generate_interactive_bom \
      --no-browser \
      --dest-dir /output/ibom \
      /workspace/Board.kicad_pcb
  "
```

### CLI オプション（主要なもの）

| オプション | 説明 |
|---|---|
| `--no-browser` | ブラウザを起動しない（**Docker では必須**） |
| `--dest-dir <dir>` | 出力先ディレクトリ（PCBファイルからの相対パス） |
| `--name-format <fmt>` | 出力ファイル名形式（%f, %p, %c, %r, %d, %D, %T） |
| `--dark-mode` | ダークモードをデフォルトにする |
| `--include-tracks` | トラック/ゾーン情報を含む（F.Cu, B.Cu のみ） |
| `--include-nets` | ネットリスト情報を含む |
| `--extra-fields <fields>` | 追加フィールド |
| `--show-fields <fields>` | BOM に表示するフィールド |
| `--group-fields <fields>` | グルーピングフィールド |
| `--exclude-dnp` | ※ `--dnp-field` で指定 |
| `--no-compression` | データ圧縮を無効化 |
| `--bom-view <view>` | bom-only, left-right (default), top-bottom |
| `--layer-view <view>` | F, FB (default), B |

### 出力

- 単一の `.html` ファイル（自己完結型）
- PCB レンダリング、BOM テーブル、インタラクティブなコンポーネントハイライトを含む
- LightStick での出力: ibom.html (194 KB)

### 依存関係

- Python 3 (Docker イメージに含まれる)
- interactivehtmlbom パッケージ (pip)
- jsonschema (自動インストール)
- xvfb (apt で追加インストール要)
- wxPython: **不要**（v2.11 CLI モード。ただし内部で wx を import 試行するため xvfb は必要）

### 既知の問題

- **KiCad 9.99 nightly との互換性**: Issue #531 で `PADSTACK` の属性変更による `AttributeError` が報告されている（v2.11.1 で修正済み）
- Docker イメージに pip が含まれないため、boardflow-action の Dockerfile で事前インストールが必要

**動作確認結果**: ✅ 成功（xvfb-run 使用）

---

## 4. Dockerfile 参考例（boardflow-action 向け）

```dockerfile
FROM kicad/kicad:9.0

USER root

# InteractiveHtmlBom 用の依存関係
RUN apt-get update -qq && \
    apt-get install -y --no-install-recommends \
      python3-pip \
      xvfb \
    && rm -rf /var/lib/apt/lists/*

# InteractiveHtmlBom をインストール
RUN pip install --break-system-packages interactivehtmlbom

# 日本語フォントが必要な場合（オプション）
# RUN apt-get update -qq && \
#     apt-get install -y --no-install-recommends fonts-noto-cjk && \
#     rm -rf /var/lib/apt/lists/*

# 作業ディレクトリ
WORKDIR /workspace

# エントリポイント（boardflow-action のスクリプト）
# ENTRYPOINT ["/action/entrypoint.sh"]
```

### コマンド実行パターン

```bash
# 1. Gerber + Drill
kicad-cli pcb export gerbers --output /output/gerbers/ /workspace/Board.kicad_pcb
kicad-cli pcb export drill --output /output/drill/ /workspace/Board.kicad_pcb

# 2. BOM (CSV)
kicad-cli sch export bom --output /output/bom.csv /workspace/Board.kicad_sch

# 3. Schematic PDF
kicad-cli sch export pdf --output /output/schematic.pdf /workspace/Board.kicad_sch

# 4. ERC (JSON)
kicad-cli sch erc --format json --severity-all --exit-code-violations \
  --output /output/erc.json /workspace/Board.kicad_sch

# 5. DRC (JSON)
kicad-cli pcb drc --format json --severity-all --exit-code-violations \
  --output /output/drc.json /workspace/Board.kicad_pcb

# 6. InteractiveHtmlBom
xvfb-run generate_interactive_bom --no-browser \
  --dest-dir /output/ibom /workspace/Board.kicad_pcb

# 7. 3D Render (top)
kicad-cli pcb render --output /output/render-top.png --side top /workspace/Board.kicad_pcb

# 8. 3D Render (bottom)
kicad-cli pcb render --output /output/render-bottom.png --side bottom /workspace/Board.kicad_pcb
```

---

## 5. 動作確認結果まとめ

| コマンド | 結果 | 備考 |
|---|---|---|
| `pcb export gerbers` | ✅ 成功 | 29 ファイル出力 |
| `pcb export drill` | ✅ 成功 | 1 ファイル (Excellon) |
| `sch export bom` | ✅ 成功 | CSV 出力 |
| `sch export pdf` | ✅ 成功 | 194 KB |
| `pcb export pdf` | ✅ 成功 | レイヤー名は `.` 区切り |
| `sch erc --format json` | ✅ 成功 | 4 violations (exit 0) |
| `sch erc --exit-code-violations` | ✅ exit 5 | 違反時 exit 5 |
| `pcb drc --format json` | ✅ 成功 | 39 violations (exit 0) |
| `pcb drc --exit-code-violations` | ✅ exit 5 | 違反時 exit 5 |
| `pcb export svg --mode-multi` | ✅ 成功 | レイヤーごと分割 |
| `pcb render` | ✅ 成功 | 54 KB PNG |
| InteractiveHtmlBom | ✅ 成功 | xvfb-run 必須 |

---

## 6. 既知の制限事項・注意点

### kicad-cli

1. **レイヤー名**: ドット区切り（`Edge.Cuts`）を使用。アンダースコア（`Edge_Cuts`）はエラーになる
2. **SVG export デフォルト動作**: KiCad 9.0 では deprecated。`--mode-multi` を明示的に指定すること
3. **DRC/ERC の exit code**: `--exit-code-violations` を付けない場合、違反があっても exit 0 を返す
4. **DRC schematic parity**: `--schematic-parity` を付けない場合、PCB とスケマティックの整合性チェックは行われない
5. **フォント**: Docker イメージには KiCad Font が含まれるが、日本語フォントは別途インストール要
6. **`pcb export gerber`（単数形）**: KiCad 9.0 で deprecated、10.0 で削除予定。`gerbers`（複数形）を使うこと

### DRC/ERC JSON スキーマ

1. **スキーマ URL**: `https://schemas.kicad.org/drc.v1.json` / `https://schemas.kicad.org/erc.v1.json`
   - 注意: KiCad Issue #23948 によると、これらの URL は実際には解決されない（リンク切れ）
2. **座標**: `coordinate_units` で指定された単位（デフォルト: mm）。`pos.x` / `pos.y` で表現
3. **violation の構造**: `type`（文字列識別子）、`description`、`severity`、`items` 配列
4. **DRC 固有**: `violations` + `unconnected_items` + `schematic_parity` の 3 配列
5. **ERC 固有**: `sheets` 配列（シートごとに violations をグループ化）

### InteractiveHtmlBom

1. **xvfb が必須**: v2.11 で wx が optional になったが、実際には X Display を要求する。`xvfb-run` で回避
2. **pip が Docker イメージに含まれない**: `python3-pip` パッケージの追加インストールが必要
3. **root 権限が必要**: apt-get / pip install のため `--user root` が必要
4. **`--break-system-packages`**: Debian 12 の PEP 668 制限により必要
5. **KiCad 9 padstack サポート**: v2.10.0 以降で対応済み
6. **`--dest-dir`**: PCB ファイルからの相対パスとして解釈される

### Docker 関連

1. **pip 不在**: boardflow-action の Dockerfile でレイヤーとして事前インストールすべき
2. **パッケージキャッシュ**: `rm -rf /var/lib/apt/lists/*` でクリーンアップ
3. **イメージサイズ最適化**: xvfb + python3-pip の追加で約 50-80 MB 増加（推定）

---

## 7. BoardFlow への示唆

### boardflow-action の設計

1. **Dockerfile**: `kicad/kicad:9.0` ベースで、python3-pip + xvfb + interactivehtmlbom を追加インストール
2. **出力ディレクトリ構造**: コマンドごとに分離（gerbers/, drill/, bom/, pdf/, erc/, drc/, ibom/, render/）
3. **DRC/ERC の扱い**:
   - `--exit-code-violations` を使用して違反の有無を検出
   - exit code 5 でも JSON レポートは生成される（abort ではない）
   - BoardFlow は JSON をパースして Issue に連携
4. **成果物一覧**:
   - Gerber + Drill → ZIP にまとめて staging に含める
   - BOM CSV → staging に含める
   - Schematic PDF → staging に含める
   - ERC/DRC JSON → staging に含める（パース用）
   - iBOM HTML → staging に含める
   - 3D Render PNG → staging に含める（サムネイル用）

### Import Worker (Issue #7) との関連

- DRC/ERC JSON のパース: 上記スキーマに基づいて Rust で deserialize
- `violations[].type` をキーとして violation 種別を分類可能
- `violations[].items[].uuid` で KiCad オブジェクトとの紐付けが可能

---

## 参照 URL

- KiCad CLI 9.0 公式ドキュメント: https://docs.kicad.org/9.0/en/cli/cli.html
- Docker Hub kicad/kicad: https://hub.docker.com/r/kicad/kicad
- kicad-cli-docker Dockerfile: https://gitlab.com/kicad/packaging/kicad-cli-docker
- InteractiveHtmlBom GitHub: https://github.com/openscopeproject/InteractiveHtmlBom
- InteractiveHtmlBom Usage Wiki: https://github.com/openscopeproject/InteractiveHtmlBom/wiki/Usage
- InteractiveHtmlBom Releases: https://github.com/openscopeproject/InteractiveHtmlBom/releases
- KiCad DRC/ERC JSON スキーマリンク Issue: https://gitlab.com/kicad/code/kicad/-/work_items/23948
