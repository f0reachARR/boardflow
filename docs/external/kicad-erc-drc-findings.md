# KiCad ERC/DRC JSON Findings フォーマットと manifest.json マッピング

Issue: #7 (追加実装)
調査日: 2026-05-01
対象: KiCad 9.0 系 (kicad-cli 9.0.8)

## 1. 要約

KiCad CLI の ERC/DRC JSON 出力フォーマットを調査し、manifest.json の `checks[].findings` 配列の型定義を決定した。KiCad JSON → manifest.json findings → `run_check_findings` テーブルへのマッピングルールを定義した。

## 2. KiCad JSON 出力スキーマ

### 2.1 共通構造体 (rc_json_schema.h)

KiCad ソースコード `include/rc_json_schema.h` (9.0 ブランチ) から取得。

```
RC_JSON::COORDINATE {
    x: f64,     // coordinate_units (デフォルト mm)
    y: f64,
}

RC_JSON::AFFECTED_ITEM {
    uuid: String,
    description: String,
    pos: COORDINATE,
}

RC_JSON::VIOLATION {
    type: String,           // rule_code (例: "clearance", "power_pin_not_driven")
    description: String,    // 人間可読な説明文
    severity: String,       // "error" | "warning" | "exclusion"
    items: [AFFECTED_ITEM], // 影響を受けるアイテム (1個以上)
    excluded: bool,         // ユーザーが除外した違反 (optional、excluded=true の場合のみ出力)
    comment: String,        // 除外コメント (optional、excluded=true の場合のみ出力)
}
```

### 2.2 ERC JSON レポート (`erc.v1.json`)

```json
{
  "$schema": "https://schemas.kicad.org/erc.v1.json",
  "coordinate_units": "mm",
  "date": "2026-04-30T13:45:40+0000",
  "kicad_version": "9.0.8",
  "source": "Board.kicad_sch",
  "included_severities": ["error", "warning", "exclusion"],
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

**構造の特徴:**
- violations は `sheets` 配列の中にネストされている
- 各 sheet には `path` (例: "/", "/SubSheet/") と `uuid_path` がある
- 1つの sheet に複数の violations が含まれる

### 2.3 DRC JSON レポート (`drc.v1.json`)

```json
{
  "$schema": "https://schemas.kicad.org/drc.v1.json",
  "coordinate_units": "mm",
  "date": "2026-04-30T13:45:52+0000",
  "kicad_version": "9.0.8",
  "source": "Board.kicad_pcb",
  "included_severities": ["error", "warning", "exclusion"],
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
  "unconnected_items": [
    {
      "type": "unconnected_items",
      "description": "Missing connection between items",
      "severity": "error",
      "items": [
        {
          "description": "Pad 1 of U1",
          "pos": { "x": 100.0, "y": 50.0 },
          "uuid": "..."
        },
        {
          "description": "Pad 3 of R1",
          "pos": { "x": 120.0, "y": 60.0 },
          "uuid": "..."
        }
      ]
    }
  ],
  "schematic_parity": []
}
```

**構造の特徴:**
- violations は **3つの配列** に分かれている:
  - `violations`: 一般的な DRC 違反
  - `unconnected_items`: 未接続アイテム (同じ VIOLATION 型)
  - `schematic_parity`: 回路図整合性チェック (同じ VIOLATION 型)
- ERC と違い sheet ネストなし、フラットな violations 配列

### 2.4 severity の値

| KiCad severity | 意味 | BoardFlow マッピング |
|---|---|---|
| `"error"` | エラー | `"error"` |
| `"warning"` | 警告 | `"warning"` |
| `"exclusion"` | ユーザーが除外した違反 | **除外** (findings に含めない) |

**注:** `--severity-all` フラグで exclusion も出力されるが、manifest.json には `excluded != true` のもののみ含める。

### 2.5 ERC violation type 一覧 (KiCad 9.0)

| type (rule_code) | 説明 | カテゴリ |
|---|---|---|
| `pin_not_connected` | Pin not connected | Connections |
| `pin_not_driven` | Input pin not driven by any Output pins | Connections |
| `power_pin_not_driven` | Input Power pin not driven by any Output Power pins | Connections |
| `no_connect_connected` | Pin with "no connection" flag is connected | Connections |
| `no_connect_dangling` | Unconnected "no connection" flag | Connections |
| `global_label_dangling` | Global label not connected anywhere else | Connections |
| `label_dangling` | Label not connected to anything | Connections |
| `single_global_label` | Global label only appears once | Connections |
| `same_local_global_label` | Local and global labels have same name | Connections |
| `wire_dangling` | Wires not connected to anything | Connections |
| `bus_entry_needed` | Bus Entry needed | Connections |
| `endpoint_off_grid` | Symbol pin or wire end off connection grid | Connections |
| `four_way_junction` | Four connection points joined together | Connections |
| `label_multiple_wires` | Label connects more than one wire | Connections |
| `unconnected_wire_endpoint` | Unconnected wire endpoint | Connections |
| `duplicate_reference` | Duplicate reference designators | Conflicts |
| `pin_to_pin` | Conflict problem between pins | Conflicts |
| `unit_value_mismatch` | Units of same symbol have different values | Conflicts |
| `different_unit_footprint` | Different footprint in another unit | Conflicts |
| `different_unit_net` | Different net on shared pin in another unit | Conflicts |
| `duplicate_sheet_names` | Duplicate sheet names | Conflicts |
| `hier_label_mismatch` | Mismatch between hierarchical labels and sheet pins | Conflicts |
| `multiple_net_names` | More than one name given to bus or net | Conflicts |
| `bus_definition_conflict` | Bus alias definition conflict | Conflicts |
| `bus_to_bus_conflict` | Buses connected but share no bus members | Conflicts |
| `bus_to_net_conflict` | Invalid bus-net connection | Conflicts |
| `net_not_bus_member` | Net graphically connected to bus but not a member | Conflicts |
| `unannotated` | Symbol is not annotated | Misc |
| `unresolved_variable` | Unresolved text variable | Misc |
| `undefined_netclass` | Undefined netclass | Misc |
| `simulation_model_issue` | SPICE model issue | Misc |
| `similar_labels` | Labels are similar (case difference) | Misc |
| `similar_power` | Power pins are similar (case difference) | Misc |
| `similar_label_and_power` | Power pin and label are similar | Misc |
| `lib_symbol_issues` | Library symbol issue | Misc |
| `lib_symbol_mismatch` | Symbol doesn't match copy in library | Misc |
| `footprint_link_issues` | Footprint link issue | Misc |
| `footprint_filter` | Assigned footprint doesn't match filters | Misc |
| `extra_units` | Symbol has more units than defined | Misc |
| `missing_unit` | Symbol has units not placed | Misc |
| `missing_input_pin` | Symbol has unplaced input pins | Misc |
| `missing_bidi_pin` | Symbol has unplaced bidirectional pins | Misc |
| `missing_power_pin` | Symbol has unplaced power input pins | Misc |
| `duplicate_pins` | Duplicate pins with different nets | Internal |
| `generic-warning` | Generic warning | Internal |
| `generic-error` | Generic error | Internal |

### 2.6 DRC violation type 一覧 (KiCad 9.0)

| type (rule_code) | 説明 | カテゴリ |
|---|---|---|
| `shorting_items` | Items shorting two nets | Electrical |
| `tracks_crossing` | Tracks crossing | Electrical |
| `clearance` | Clearance violation | Electrical |
| `creepage` | Creepage violation | Electrical |
| `via_dangling` | Via not connected | Electrical |
| `track_dangling` | Track has unconnected end | Electrical |
| `starved_thermal` | Thermal relief incomplete | Electrical |
| `copper_edge_clearance` | Board edge clearance violation | DFM |
| `hole_clearance` | Hole clearance violation | DFM |
| `hole_to_hole` | Drilled hole too close | DFM |
| `holes_co_located` | Drilled holes co-located | DFM |
| `track_width` | Track width | DFM |
| `track_angle` | Track angle | DFM |
| `track_segment_length` | Track segment length | DFM |
| `annular_width` | Annular width | DFM |
| `drill_out_of_range` | Hole size out of range | DFM |
| `via_diameter` | Via diameter | DFM |
| `padstack` | Padstack is questionable | DFM |
| `microvia_drill_out_of_range` | Micro via hole size out of range | DFM |
| `courtyards_overlap` | Courtyards overlap | DFM |
| `missing_courtyard` | Footprint has no courtyard | DFM |
| `malformed_courtyard` | Footprint has malformed courtyard | DFM |
| `invalid_outline` | Board has malformed outline | DFM |
| `copper_sliver` | Copper sliver | DFM |
| `solder_mask_bridge` | Solder mask aperture bridges items | DFM |
| `connection_width` | Copper connection too narrow | DFM |
| `duplicate_footprints` | Duplicate footprints | Schematic Parity |
| `missing_footprint` | Missing footprint | Schematic Parity |
| `extra_footprint` | Extra footprint | Schematic Parity |
| `footprint_symbol_mismatch` | Footprint attributes don't match symbol | Schematic Parity |
| `footprint_filters_mismatch` | Footprint doesn't match filters | Schematic Parity |
| `net_conflict` | Pad net doesn't match schematic | Schematic Parity |
| `unconnected_items` | Missing connection between items | Schematic Parity |
| `length_out_of_range` | Track length out of range | Signal Integrity |
| `skew_out_of_range` | Skew between tracks out of range | Signal Integrity |
| `too_many_vias` | Too many or too few vias | Signal Integrity |
| `diff_pair_gap_out_of_range` | Differential pair gap out of range | Signal Integrity |
| `diff_pair_uncoupled_length_too_long` | Differential uncoupled length too long | Signal Integrity |
| `silk_overlap` | Silkscreen overlap | Readability |
| `silk_over_copper` | Silkscreen clipped by solder mask | Readability |
| `silk_edge_clearance` | Silkscreen clipped by board edge | Readability |
| `text_height` | Text height out of range | Readability |
| `text_thickness` | Text thickness out of range | Readability |
| `mirrored_text_on_front_layer` | Mirrored text on front layer | Readability |
| `nonmirrored_text_on_back_layer` | Non-mirrored text on back layer | Readability |
| `items_not_allowed` | Items not allowed | Misc |
| `text_on_edge_cuts` | Text on Edge.Cuts layer | Misc |
| `zones_intersect` | Copper zones intersect | Misc |
| `isolated_copper` | Isolated copper fill | Misc |
| `footprint` | Footprint is not valid | Misc |
| `pth_inside_courtyard` | PTH inside courtyard | Misc |
| `npth_inside_courtyard` | NPTH inside courtyard | Misc |
| `item_on_disabled_layer` | Item on disabled copper layer | Misc |
| `unresolved_variable` | Unresolved text variable | Misc |
| `footprint_type_mismatch` | Footprint type doesn't match pads | Misc |
| `lib_footprint_issues` | Footprint not found in libraries | Misc |
| `lib_footprint_mismatch` | Footprint doesn't match library copy | Misc |
| `through_hole_pad_without_hole` | Through hole pad has no hole | Misc |
| `assertion_failure` | Assertion failure | Misc |
| `generic_warning` | Warning | Internal |
| `generic_error` | Error | Internal |

## 3. manifest.json findings フィールド設計

### 3.1 ManifestFinding 型定義

```rust
/// manifest.json の checks[].findings 配列の各要素
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFinding {
    /// "error" | "warning" | "notice"
    pub severity: String,
    /// KiCad violation type (例: "clearance", "power_pin_not_driven")
    pub rule_code: String,
    /// 人間可読なタイトル (KiCad violation.description)
    pub title: String,
    /// 詳細メッセージ (affected items の description を改行区切りで結合)
    #[serde(default)]
    pub message: Option<String>,
    /// "schematic" | "pcb" | "net" | "footprint" | "symbol"
    #[serde(default)]
    pub subject_kind: Option<String>,
    /// 主要な参照先 (最初の affected item の description)
    #[serde(default)]
    pub subject_ref: Option<String>,
    /// ERC の sheet path (例: "/", "/SubSheet/")。DRC は null
    #[serde(default)]
    pub sheet_path: Option<String>,
    /// PCB レイヤー名。KiCad JSON には含まれないため通常 null
    #[serde(default)]
    pub pcb_layer: Option<String>,
    /// 最初の affected item の位置 (mm)
    #[serde(default)]
    pub pos_mm: Option<CoordinateMm>,
    /// 生の KiCad VIOLATION オブジェクト (将来の拡張用)
    #[serde(default)]
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinateMm {
    pub x: f64,
    pub y: f64,
}
```

### 3.2 JSON スキーマ例

```json
{
  "checks": [
    {
      "kind": "erc",
      "status": "failed",
      "error_count": 2,
      "warning_count": 1,
      "notice_count": 0,
      "tool_name": "kicad-cli",
      "tool_version": "9.0.8",
      "raw_summary": {
        "$schema": "https://schemas.kicad.org/erc.v1.json",
        "coordinate_units": "mm",
        "kicad_version": "9.0.8"
      },
      "findings": [
        {
          "severity": "error",
          "rule_code": "power_pin_not_driven",
          "title": "Input Power pin not driven by any Output Power pins",
          "message": "Symbol #PWR019 Pin 1 [Power input, Line]",
          "subject_kind": "schematic",
          "subject_ref": "#PWR019",
          "sheet_path": "/",
          "pos_mm": { "x": 0.5715, "y": 0.2667 },
          "raw": {
            "type": "power_pin_not_driven",
            "description": "Input Power pin not driven by any Output Power pins",
            "severity": "error",
            "items": [
              {
                "description": "Symbol #PWR019 Pin 1 [Power input, Line]",
                "pos": { "x": 0.5715, "y": 0.2667 },
                "uuid": "..."
              }
            ]
          }
        }
      ]
    },
    {
      "kind": "drc",
      "status": "failed",
      "error_count": 5,
      "warning_count": 3,
      "notice_count": 0,
      "tool_name": "kicad-cli",
      "tool_version": "9.0.8",
      "raw_summary": {
        "$schema": "https://schemas.kicad.org/drc.v1.json",
        "coordinate_units": "mm",
        "kicad_version": "9.0.8"
      },
      "findings": [
        {
          "severity": "error",
          "rule_code": "items_not_allowed",
          "title": "Items not allowed (keepout area)",
          "message": "Footprint SW2 at (194.00 mm, 119.25 mm)",
          "subject_kind": "pcb",
          "subject_ref": "Footprint SW2",
          "sheet_path": null,
          "pos_mm": { "x": 194.0, "y": 119.25 },
          "raw": { "..." : "..." }
        },
        {
          "severity": "error",
          "rule_code": "unconnected_items",
          "title": "Missing connection between items",
          "message": "Pad 1 of U1 → Pad 3 of R1",
          "subject_kind": "net",
          "subject_ref": "Pad 1 of U1",
          "sheet_path": null,
          "pos_mm": { "x": 100.0, "y": 50.0 },
          "raw": { "..." : "..." }
        }
      ]
    }
  ]
}
```

## 4. マッピングルール

### 4.1 KiCad JSON → manifest.json findings

#### ERC

```
for each sheet in erc_report.sheets:
    for each violation in sheet.violations:
        if violation.excluded == true:
            skip  # 除外された violations は含めない
        finding = ManifestFinding {
            severity: map_severity(violation.severity),
            rule_code: violation.type,
            title: violation.description,
            message: violation.items.map(|i| i.description).join("\n"),
            subject_kind: "schematic",
            subject_ref: extract_subject_ref(violation.items[0].description),
            sheet_path: sheet.path,
            pcb_layer: null,
            pos_mm: violation.items[0].pos,  # 最初のアイテムの位置
            raw: violation (as JSON),
        }
```

#### DRC

```
# violations, unconnected_items, schematic_parity の 3 配列を結合
for each violation in drc_report.violations
                    ++ drc_report.unconnected_items
                    ++ drc_report.schematic_parity:
    if violation.excluded == true:
        skip
    finding = ManifestFinding {
        severity: map_severity(violation.severity),
        rule_code: violation.type,
        title: violation.description,
        message: violation.items.map(|i| i.description).join("\n"),
        subject_kind: infer_drc_subject_kind(violation),
        subject_ref: extract_subject_ref(violation.items[0].description),
        sheet_path: null,
        pcb_layer: null,
        pos_mm: violation.items[0].pos,
        raw: violation (as JSON),
    }
```

#### severity マッピング

```
fn map_severity(kicad_severity: &str) -> &str {
    match kicad_severity {
        "error" => "error",
        "warning" => "warning",
        _ => "notice",  // fallback (通常到達しない)
    }
}
```

#### subject_kind 推定ルール

```
fn infer_drc_subject_kind(violation) -> &str {
    match violation.type {
        // unconnected_items 配列由来
        "unconnected_items" => "net",
        // footprint 関連
        "courtyards_overlap" | "missing_courtyard" | "malformed_courtyard"
        | "pth_inside_courtyard" | "npth_inside_courtyard"
        | "duplicate_footprints" | "missing_footprint" | "extra_footprint"
        | "footprint_symbol_mismatch" | "footprint_filters_mismatch"
        | "footprint" | "footprint_type_mismatch"
        | "lib_footprint_issues" | "lib_footprint_mismatch" => "footprint",
        // net 関連
        "net_conflict" | "shorting_items" => "net",
        // デフォルト: pcb
        _ => "pcb",
    }
}
```

#### subject_ref 抽出ルール

```
fn extract_subject_ref(item_description: &str) -> Option<String> {
    // KiCad の affected item description 例:
    //   "Footprint SW2"  → "SW2"
    //   "Pad 1 of U1"    → "U1"
    //   "Symbol #PWR019 Pin 1 [Power input, Line]" → "#PWR019"
    //   "Segment on Edge.Cuts" → null
    //
    // ヒューリスティック: 主要なリファレンス識別子を抽出
    // 簡略版は description 全体を subject_ref として使用
    Some(item_description.to_string())
}
```

### 4.2 manifest.json findings → run_check_findings テーブル

| manifest.json field | DB column | 変換 |
|---|---|---|
| (auto-generated) | `id` | `Uuid::now_v7()` |
| (from parent check) | `run_check_id` | 親 `run_checks.id` |
| `severity` | `severity` | そのまま ("error" / "warning" / "notice") |
| `rule_code` | `rule_code` | そのまま |
| `title` | `title` | そのまま |
| `message` | `message` | そのまま |
| `subject_kind` | `subject_kind` | そのまま ("schematic" / "pcb" / "net" / "footprint" / "symbol") |
| `subject_ref` | `subject_ref` | そのまま |
| `sheet_path` | `sheet_path` | そのまま |
| `pcb_layer` | `pcb_layer` | そのまま (通常 null) |
| `pos_mm.x` | `x_um` | `(pos_mm.x * 1000.0).round() as i32` (mm → µm) |
| `pos_mm.y` | `y_um` | `(pos_mm.y * 1000.0).round() as i32` (mm → µm) |
| (なし) | `bbox_json` | null (KiCad JSON に bbox 情報なし) |
| `raw` | `raw_payload_json` | そのまま JSONB として保存 |
| (auto-incrementing) | `sort_index` | findings 配列内の 0-based インデックス |
| (auto-generated) | `created_at` | `Utc::now()` |

### 4.3 座標変換の注意事項

- KiCad JSON: `coordinate_units` フィールドが "mm" であることを確認
- `--units mm` がデフォルト (kicad-cli 9.0)
- DB の `x_um`, `y_um`: マイクロメートル (整数)
- 変換: `mm * 1000 → µm` (例: 194.0 mm → 194000 µm)
- `pos_mm` が null の場合 (items が空): `x_um`, `y_um` は null

## 5. BoardFlow への示唆

### 5.1 ManifestCheck 構造体 (実装確定版)

`ManifestCheck.findings` は **`Vec<serde_json::Value>`** として定義する。個別 finding を typed struct ではなく生 JSON で保持することで、1 件の malformed finding が manifest 全体のデシリアライズを阻害しない設計にしている。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCheck {
    pub kind: String,
    pub status: String,
    #[serde(default)]
    pub error_count: i32,
    #[serde(default)]
    pub warning_count: i32,
    #[serde(default)]
    pub notice_count: i32,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_version: Option<String>,
    #[serde(default)]
    pub raw_summary: Option<serde_json::Value>,
    /// findings は Vec<serde_json::Value> とし、Worker 側で個別にデシリアライズする
    #[serde(default)]
    pub findings: Vec<serde_json::Value>,
}
```

Worker が findings を処理する際に使う typed struct (`ManifestFinding`) は以下:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFinding {
    pub severity: String,
    pub rule_code: String,
    pub title: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub subject_kind: Option<String>,
    #[serde(default)]
    pub subject_ref: Option<String>,
    #[serde(default)]
    pub sheet_path: Option<String>,
    #[serde(default)]
    pub pcb_layer: Option<String>,
    #[serde(default)]
    pub pos_mm: Option<CoordinateMm>,
    #[serde(default)]
    pub raw: Option<serde_json::Value>,
}
```

### 5.2 Worker (crates/worker) の findings 保存フロー

Worker は findings を **個別にデシリアライズ** し、INSERT 前に **正規化** を行う:

```rust
for check in &manifest.checks {
    let check_id = Uuid::now_v7();
    run_check::insert(&mut *tx, check_id, board_run_id, ...);

    for (idx, raw_finding) in check.findings.iter().enumerate() {
        // 1. 個別デシリアライズを試行
        match serde_json::from_value::<ManifestFinding>(raw_finding.clone()) {
            Ok(finding) => {
                // 2. INSERT前にDB制約に合わせて正規化
                let severity = normalize_severity(&finding.severity);
                let subject_kind = normalize_subject_kind(finding.subject_kind.as_deref());
                let x_um = finding.pos_mm.as_ref().map(|p| (p.x * 1000.0).round() as i32);
                let y_um = finding.pos_mm.as_ref().map(|p| (p.y * 1000.0).round() as i32);

                run_check_finding::insert(
                    &mut *tx, Uuid::now_v7(), check_id,
                    severity, Some(&finding.rule_code), Some(&finding.title),
                    finding.message.as_deref(),
                    subject_kind, finding.subject_ref.as_deref(),
                    finding.sheet_path.as_deref(), finding.pcb_layer.as_deref(),
                    x_um, y_um, None,
                    finding.raw.as_ref().or(Some(raw_finding)),
                    idx as i32,
                ).await?;
            }
            Err(_) => {
                // 3. パース失敗: severity="notice" + raw_payload_json に生データ保存
                run_check_finding::insert(
                    &mut *tx, Uuid::now_v7(), check_id,
                    "notice", None, None, None,
                    None, None, None, None,
                    None, None, None,
                    Some(raw_finding),
                    idx as i32,
                ).await?;
            }
        }
    }
}
```

### 5.3 INSERT前の正規化ルール

DB の CHECK 制約違反を未然に防止するため、Worker は INSERT 前に以下の正規化を行う。正規化で変換された元の値は `raw_payload_json` (finding の `raw` フィールド) 側にのみ残る。

#### severity 正規化

```rust
fn normalize_severity(s: &str) -> &str {
    match s {
        "error" | "warning" | "notice" => s,
        _ => "notice",  // 不明な severity は "notice" にフォールバック
    }
}
```

DB CHECK 制約: `severity IN ('error', 'warning', 'notice')`

#### subject_kind 正規化

```rust
fn normalize_subject_kind(s: Option<&str>) -> Option<&str> {
    match s {
        Some("schematic" | "pcb" | "net" | "footprint" | "symbol") => s,
        _ => None,  // 不明な subject_kind は None にフォールバック
    }
}
```

DB CHECK 制約: `subject_kind IN ('schematic', 'pcb', 'net', 'footprint', 'symbol')` (nullable)

### 5.4 パース失敗時の挙動

- finding の `serde_json::from_value::<ManifestFinding>()` が失敗した場合:
  - `severity = "notice"` (DB CHECK 制約安全値)
  - `rule_code`, `title`, `message`, `subject_kind`, `subject_ref` 等は全て NULL
  - `raw_payload_json` に元の JSON Value をそのまま保存
  - 処理は中断せず、次の finding に進む
- 正規化で `severity` が変換された場合:
  - 変換前の値は `raw_payload_json` 内の元データ (`raw` フィールド) にのみ残る
  - DB の `severity` カラムには正規化後の安全な値が入る

### 5.5 DB クエリ (crates/db)

`run_check_finding::insert` クエリ:

```sql
INSERT INTO run_check_findings (
    id, run_check_id, severity, rule_code, title, message,
    subject_kind, subject_ref, sheet_path, pcb_layer,
    x_um, y_um, bbox_json, raw_payload_json,
    sort_index, created_at
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW())
RETURNING *
```

### 5.6 GitHub Action 側の変換ロジック

Action は以下を実行:
1. `kicad-cli sch erc --format json --severity-all --output erc.json`
2. `kicad-cli pcb drc --format json --severity-all --output drc.json`
3. erc.json / drc.json を解析し、manifest.json の `checks[].findings` を生成
4. `excluded == true` の violations を除外
5. `coordinate_units` が "mm" であることを検証

## 6. 採用/不採用判断

**採用**: 上記の manifest.json findings 設計を採用する。

理由:
- KiCad JSON の VIOLATION 構造はシンプルで安定している (v1 スキーマ)
- 1 VIOLATION = 1 finding のマッピングが自然
- `raw` フィールドで将来の拡張に対応可能
- 座標は mm → µm の単純な乗算で変換可能
- `excluded` violations のフィルタリングで不要な findings を除外

## 7. 制約と pitfall

1. **`pcb_layer` が KiCad JSON に含まれない**: DRC violation の位置情報はあるが、レイヤー情報は JSON に出力されない。将来 KiCad が拡張する可能性があるため `pcb_layer` フィールドは保持するが、現時点では常に null
2. **`bbox_json` が KiCad JSON に含まれない**: 同様に bounding box 情報は出力されない。null で保存
3. **`coordinate_units` の検証**: Action 側で "mm" であることを確認する。`--units` オプションを明示的に指定することを推奨
4. **ERC の sheet path**: 階層回路図では "/" 以外の path が出現する。sheet_path フィールドでこれを保持
5. **DRC の 3 配列**: violations, unconnected_items, schematic_parity を全て処理する必要がある。見落としやすい
6. **exclusion の扱い**: `--severity-all` は excluded も含むが、manifest.json には含めない。Action 側でフィルタする
7. **KiCad 10.0 互換性**: `$schema` URL は v1 のまま。フィールド追加の可能性があるが、`#[serde(default)]` でforward-compatible
8. **`$schema` URL が壊れている**: GitLab Issue #23948 によると、schemas.kicad.org のスキーマ URL が実際にはアクセス不能。スキーマ検証には使わない
9. **items 配列が空の場合**: 通常 1 個以上だが、空の場合は pos_mm = null, subject_ref = null として処理
10. **大量 findings のパフォーマンス**: 大規模プロジェクトでは数百〜数千の findings が発生しうる。バッチ INSERT を検討

## 8. 未解決の疑問

1. **findings 上限数**: 非常に大量の violations があるプロジェクトで全件保存するか、上限を設けるか → MVP では全件保存 (上限設定は後で検討)
2. **notice severity の運用**: KiCad には "notice" severity がない。将来の拡張用に残すが、当面は error/warning のみ

## 9. 参照 URL

- KiCad ソースコード (9.0 ブランチ):
  - `rc_json_schema.h`: https://gitlab.com/kicad/code/kicad/-/blob/9.0/include/rc_json_schema.h
  - `drc_item.cpp`: https://gitlab.com/kicad/code/kicad/-/blob/9.0/pcbnew/drc/drc_item.cpp
  - `erc_item.cpp`: https://gitlab.com/kicad/code/kicad/-/blob/9.0/eeschema/erc/erc_item.cpp
- KiCad CLI ドキュメント (9.0): https://docs.kicad.org/9.0/en/cli/cli.html
- KiCad JSON スキーマ (参考、URL が壊れている): https://schemas.kicad.org/erc.v1.json, https://schemas.kicad.org/drc.v1.json
- GitLab Issue #23948 (スキーマ URL 壊れ): https://gitlab.com/kicad/code/kicad/-/issues/23948
- GitLab Issue #22330 (DRC exclusion comment 欠落): https://gitlab.com/kicad/code/kicad/-/issues/22330
