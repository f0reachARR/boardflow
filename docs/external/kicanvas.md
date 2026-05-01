# KiCanvas 調査メモ

## 1. 要約

KiCanvas は KiCad の回路図と基板をブラウザ上で表示するための、オープンソースのインタラクティブビューアである。公式 KiCad サイトにも外部ツールとして掲載されており、実装は TypeScript、描画は Canvas / WebGL、UI は Web Components で構成される。

BoardFlow MVP では、KiCanvas を「KiCad生ファイルの認可付きプレビュー」として正式採用する。
現在仕様にある `schematic_pdf`、`pcb_top_svg`、`pcb_bottom_svg`、`pcb_pdf` は安定した静的プレビューとして残しつつ、追加で `.kicad_sch`、`.kicad_pcb`、必要に応じて `.kicad_pro` を artifact として保存し、Run詳細や BoardProject ページの `Schematic` / `PCB Preview` タブで KiCanvas 表示を提供する。

ただし KiCanvas は alpha と明記され、APIや未実装機能が多い。MVP の主要導線を KiCanvas のみに依存させるのは避け、PDF/SVG/iBOM をフォールバック兼正本プレビューとして維持する。

## 2. 確認した情報

### 基本

- KiCanvas は KiCad schematics / boards のブラウザビューア。
- Canvas と WebGL を使って描画する。
- Web Components として提供され、React / Next.js など特定フレームワークへの専用統合は不要という方針。
- runtime dependency はなく、必要なものを bundle する設計。
- GitHub README では early alpha とされ、既知の不具合や未実装がある。
- GitHub 上に release は公開されていない状態だったため、MVPでは npm package として安定導入できる前提にしない方がよい。
- `package.json` の license は MIT。

### 埋め込み API

KiCanvas の埋め込みは `<kicanvas-embed>` custom element が中心。

単一ファイル:

```html
<kicanvas-embed src="my-schematic.kicad_sch"></kicanvas-embed>
```

基本操作付き:

```html
<kicanvas-embed
  src="my-schematic.kicad_sch"
  controls="basic"
  controlslist="nodownload">
</kicanvas-embed>
```

複数ファイル:

```html
<kicanvas-embed controls="full" controlslist="nodownload">
  <kicanvas-source src="project.kicad_pro"></kicanvas-source>
  <kicanvas-source src="main.kicad_sch"></kicanvas-source>
  <kicanvas-source src="board.kicad_pcb"></kicanvas-source>
</kicanvas-embed>
```

ドキュメント上の複数ファイル例では project file として `project.kicad_prj` が出ているが、KiCad の現行 project file は `.kicad_pro` なので、BoardFlow では `.kicad_pro` を artifact として扱う前提で検証する。

### 主な属性

- `src`: 表示する KiCad document URL。
- `controls`: `none` / `basic` / `full`。
- `controlslist`: `nodownload`、`noflipview` などで UI を一部抑制。
- `type`: inline source の種別。`schematic` / `board` / `project` / `worksheet`。
- `name`: inline source のファイル名。複数ファイルや階層回路図の関連付けに重要。
- `theme`、`zoom` は未実装または不完全とされている。

イベント API、deep linking、いくつかの panels / controls は未実装または不完全とされている。BoardFlow MVP では、KiCanvas 内部イベントに依存した機能を設計しない。

### 対応と制限

- KiCad 6以降のファイルを主対象にしており、KiCad 5 files は非対応とされる。
- 一部 KiCad 7 features は未完全実装の可能性がある。
- 自動テスト対象ブラウザは desktop Chrome / Firefox / Safari が中心で、それ以外は問題が出る可能性がある。
- non-goals として、編集、offline rendering、3D board rendering、server-side usage、comparison / visual diffing、特定フロントエンドフレームワーク統合が挙げられている。

## 3. BoardFlow MVP で使えそうな部分

### 3.1 Web UI の追加プレビュー

既存仕様の `BoardProjectページ` は以下のタブを持つ。

```text
Overview
Schematic
PCB Preview
iBOM
BOM
Fabrication
Checks
Diff
Runs
History
```

KiCanvas は `Schematic` と `PCB Preview` に入れるのが自然。

- `Schematic`: `kicad_sch` artifact が `available` の場合に KiCanvas で回路図を表示する。
- `PCB Preview`: `kicad_pcb` artifact が `available` の場合に KiCanvas で基板を表示する。
- 複数ファイルが揃っている場合は、`kicanvas-source` を複数渡して `controls="full"` の project viewer に寄せる。
- KiCanvas が読み込めない場合、既存の PDF / SVG プレビューとダウンロード導線を表示する。

### 3.2 Artifact type

MVPではPDF / SVG / iBOM / BOM / 製造ファイル / check report に加えて、KiCad の生ファイルをartifactとして保存する。

追加するartifact typeは以下。

```text
kicad_project
kicad_schematic
kicad_pcb
kicad_worksheet
```

`kicad_worksheet` は `.kicad_wks` が存在する場合のみ optional とする。

階層回路図を正しく表示したい場合、root schematic だけでなく project directory 配下の関連 `.kicad_sch` を複数 artifact として保存する必要がある。MVP ではまず「同一 project_dir 配下の `.kicad_sch` と `.kicad_pcb` と `.kicad_pro` を viewer source として登録する」方針が扱いやすい。

### 3.3 bundle レイアウト追加案

既存の bundle:

```text
bundle.zip
  manifest.json
  review/
  assembly/
  fabrication/
  checks/
  diff/
```

KiCanvas 用に以下を追加する。

```text
kicad/
  project.kicad_pro
  main.kicad_sch
  sub_sheet_1.kicad_sch
  board.kicad_pcb
  drawing.kicad_wks
```

Action は元ファイル名をそのまま維持する。KiCanvas は project / schematic / worksheet の関連付けでファイル名を使う可能性があるため、単純な正規化名へのリネームは避ける。

manifest の artifact 例:

```json
{
  "type": "kicad_pcb",
  "status": "available",
  "path": "kicad/hardware/motor_driver/motor_driver.kicad_pcb",
  "content_type": "text/plain; charset=utf-8",
  "sha256": "sha256:...",
  "size_bytes": 123456
}
```

`path` は zip 内では `kicad/` 以下に閉じ込め、元の project_dir 相対構造を保つ。import worker は path traversal、絶対パス、過大サイズを既存 artifact と同様に拒否する。

### 3.4 配信 API

KiCanvas はブラウザ上で `src` URL からファイルを読み込む。private artifact 前提の BoardFlow では、以下のどちらかが必要。

- artifact proxy が認可後に `.kicad_sch` / `.kicad_pcb` / `.kicad_pro` を返す。
- backend が短命 URL を発行し、KiCanvas の `src` / `kicanvas-source src` に渡す。

複数ファイル表示では、viewer 表示直前に必要ファイル一式の artifact proxy URL を取得する。
KiCanvas専用APIは作らず、MVPでは他のpreview/download用途も含む汎用APIに寄せる。

例:

```http
GET /api/v1/board-runs/{board_run_id}/viewer-sources
```

レスポンス例:

```json
{
  "board_run_id": "br_abc123",
  "expires_at": "2030-01-01T12:00:00Z",
  "viewers": {
    "kicanvas": {
      "status": "available",
      "sources": [
        {
          "kind": "project",
          "name": "motor_driver.kicad_pro",
          "source_path": "hardware/motor_driver/motor_driver.kicad_pro",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_project?token=eyJ..."
        },
        {
          "kind": "schematic",
          "name": "motor_driver.kicad_sch",
          "source_path": "hardware/motor_driver/motor_driver.kicad_sch",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_schematic?token=eyJ..."
        },
        {
          "kind": "board",
          "name": "motor_driver.kicad_pcb",
          "source_path": "hardware/motor_driver/motor_driver.kicad_pcb",
          "url": "https://artifacts.boardflow.example.com/proxy/artifacts/art_board?token=eyJ..."
        }
      ]
    }
  }
}
```

### 3.5 Next.js での扱い

KiCanvas は custom element なので、Next.js では Client Component として閉じ込める。

- `kicanvas.js` はアプリの `public/vendor/kicanvas/kicanvas.js` のように vendoring する。
- `next/script` または dynamic import で browser のみ読み込む。
- TypeScript では `kicanvas-embed` / `kicanvas-source` の JSX IntrinsicElements 型定義を追加する。
- Server Components は artifact 状態や署名URL発行の入口までを担当し、実際の viewer は Client Component にする。

導入イメージ:

```tsx
"use client";

import Script from "next/script";

type Source = {
  type: "project" | "schematic" | "board" | "worksheet";
  name: string;
  url: string;
};

export function KiCanvasViewer({ sources }: { sources: Source[] }) {
  return (
    <>
      <Script type="module" src="/vendor/kicanvas/kicanvas.js" />
      <kicanvas-embed controls="full" controlslist="nodownload">
        {sources.map((source) => (
          <kicanvas-source
            key={`${source.type}:${source.name}`}
            src={source.url}
            type={source.type}
            name={source.name}
          />
        ))}
      </kicanvas-embed>
    </>
  );
}
```

## 4. 仕様への反映案

### 4.1 `8.2 生成する成果物`

MVP default outputs に KiCanvas 用の元ファイル保存を追加する。

```text
kicad/
  *.kicad_pro
  *.kicad_sch
  *.kicad_pcb
  *.kicad_wks
```

ただし、これは KiCad を実行して生成する成果物ではなく、Action が検出済み project_dir からコピーして bundle に含める閲覧用 source artifact と位置付ける。

### 4.2 `8.3 成果物種別`

追加するartifact type:

```text
kicad_project
kicad_schematic
kicad_pcb
kicad_worksheet
```

`kicad_schematic` は複数行を許容する。
DB の `artifacts` は `type` だけでは一意にせず、`logical_name` または repository root 相対の `source_path` を別カラムとして持つ。

### 4.3 `14.4 PCB Preview`

表示内容を以下に拡張する。

```text
- KiCanvas による .kicad_pcb interactive preview
- 表面SVG
- 裏面SVG
- PDFリンク
```

KiCanvas preview が unavailable / failed の場合も、SVG / PDF は通常通り使えるようにする。

### 4.4 `14.2 BoardProjectページ`

`Schematic` タブでは KiCanvas を第一候補にし、`schematic_pdf` を fallback とする。

```text
Schematic
  - KiCanvas による .kicad_sch interactive preview
  - schematic PDF fallback
```

### 4.5 `7.4 MVPで比較する内容`

KiCanvas は comparison / visual diffing を non-goal としている。MVP の差分は既存方針どおり、SVG/PDFへの導線と軽量サマリに留める。

```text
- KiCanvas は差分描画には使わない
- Diff画面から base/head それぞれの KiCanvas viewer へ遷移できる導線を置く
```

## 5. セキュリティと運用上の注意

- KiCad source artifact は private design data なので、GitHub Issue には直接URLを載せない。
- KiCanvas の download control は `controlslist="nodownload"` で隠す。ただしブラウザがファイルを取得する以上、閲覧権限を持つユーザーによる取得自体は防げない。これは「UI上の誤操作防止」として扱う。
- artifact response は既存方針どおり `Content-Security-Policy`、制限付き `Access-Control-Allow-Origin`、`X-Content-Type-Options: nosniff` を設定する。
- KiCanvas 用 source は `text/plain; charset=utf-8` または専用 content type を使い、HTML として解釈させない。
- KiCanvas 自体はアプリに vendoring し、外部 CDN から都度読み込まない。private artifact を扱う画面で第三者 origin の script を避ける。
- KiCanvas は WebGL を使うため、ブラウザやGPU環境で表示失敗する可能性がある。必ず静的 PDF/SVG fallback を残す。

## 6. MVP 採用判断

MVPでは以下の範囲で採用する。

```text
MVPに含める:
- KiCanvas bundle script の vendoring
- kicad_project / kicad_schematic / kicad_pcb artifact 保存
- viewer-sources API 経由での短命URL取得
- Run詳細またはBoardProject詳細での interactive preview
- PDF/SVG fallback
- Playwright smoke test で viewer コンテナが表示されることを確認

MVPに含めない:
- KiCanvas events を使った選択同期
- deep link
- visual diff / overlay
- 3D preview
- server-side rendering
- KiCanvas を使った画像生成
- GitHub IssueへのKiCanvas直接埋め込み
```

結論として、KiCanvas は BoardFlow の「生成済み成果物をWebで見やすく共有する」という目的にかなり合う。ただし alpha であるため、MVPでは補助的な interactive viewer として採用し、静的 artifact と既存の artifact 状態管理を主軸に置く。

## 7. 参照元

- KiCanvas embedding documentation: https://kicanvas.org/embedding/
- KiCanvas GitHub repository: https://github.com/theacodes/kicanvas
- KiCanvas roadmap: https://kicanvas.org/roadmap/
- KiCanvas development documentation: https://kicanvas.org/development/
- KiCad external tools entry: https://www.kicad.org/external-tools/kicanvas/
- Context7: `/theacodes/kicanvas`
