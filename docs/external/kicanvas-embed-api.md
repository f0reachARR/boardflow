# KiCanvas `<kicanvas-embed>` Web Component API

## 要約

KiCanvas の `<kicanvas-embed>` custom element は、`src`・`controls`・`controlslist` 属性で基本的なビューア制御を提供する。複数ファイル表示は `<kicanvas-source>` 子要素で行う。Events API は全て未実装。alpha 段階であり API は変更される可能性がある。

## 確認した情報

### 基本使用法

```html
<!-- 単一ファイル -->
<kicanvas-embed src="my-schematic.kicad_sch"></kicanvas-embed>

<!-- 基本操作付き -->
<kicanvas-embed src="my-schematic.kicad_sch" controls="basic"></kicanvas-embed>

<!-- フル UI -->
<kicanvas-embed src="my-schematic.kicad_sch" controls="full"></kicanvas-embed>

<!-- ダウンロードボタン非表示 -->
<kicanvas-embed src="my-schematic.kicad_sch" controls="basic" controlslist="nodownload"></kicanvas-embed>
```

### 複数ファイル（プロジェクト表示）

```html
<kicanvas-embed controls="full">
    <kicanvas-source src="project.kicad_prj"></kicanvas-source>
    <kicanvas-source src="schematic1.kicad_sch"></kicanvas-source>
    <kicanvas-source src="schematic2.kicad_sch"></kicanvas-source>
    <kicanvas-source src="board.kicad_pcb"></kicanvas-source>
</kicanvas-embed>
```

- project file があると、root schematic がデフォルト表示される
- project file がない場合は最初の schematic が表示される
- 右側の project panel でファイル切り替え可能

### インライン source

```html
<kicanvas-embed>
    <kicanvas-source>
        (kicad_sch (version 20230121) ...)
    </kicanvas-source>
</kicanvas-embed>
```

### 属性一覧

#### `<kicanvas-embed>` の属性

| 属性 | 値 | 説明 | 状態 |
|---|---|---|---|
| `src` | URL | 表示するドキュメントの URL | 実装済み |
| `controls` | `none` / `basic` / `full` | インタラクティビティレベル。`none` は img のように動作（デフォルト） | 実装済み |
| `controlslist` | スペース区切り | UI のカスタマイズ | 一部未実装 |
| `type` | `schematic` / `board` / `project` / `worksheet` | inline source のファイル種別を明示 | 実装済み |
| `name` | ファイル名 | inline source のファイル名。階層回路図のリンクに必要 | 実装済み |
| `theme` | `kicad` / `witchhazel` | カラーテーマ | ⚠️ 未実装/不完全 |
| `zoom` | `objects` / `page` / `x y w h` / reference list | 初期ビュー | ⚠️ 未実装/不完全 |

#### `controlslist` の有効値

| 値 | 説明 | 状態 |
|---|---|---|
| `nooverlay` | "click or tap to interact" オーバーレイを非表示 | 実装済み |
| `nofullscreen` | フルスクリーンボタンを非表示 | ⚠️ 未実装 |
| `nodownload` | ダウンロードボタンを非表示 | 実装済み |
| `download` | `controls="none"` でもダウンロードボタンを表示 | 実装済み |
| `noflipview` | flip board ボタンを非表示 | 実装済み |
| `flipview` | `controls="none"` でも flip board ボタンを表示 | 実装済み |
| `nosymbols` | schematic symbols パネルを非表示 | ⚠️ 未実装 |
| `nofootprints` | board footprints パネルを非表示 | ⚠️ 未実装 |
| `noobjects` | board objects パネルを非表示 | ⚠️ 未実装 |
| `noproperties` | selection properties パネルを非表示 | ⚠️ 未実装 |
| `noinfo` | document info パネルを非表示 | ⚠️ 未実装 |
| `nopreferences` | user preferences パネルを非表示 | ⚠️ 未実装 |
| `nohelp` | help パネルを非表示 | ⚠️ 未実装 |

#### `<kicanvas-source>` の属性

| 属性 | 値 | 説明 |
|---|---|---|
| `src` | URL | ソースファイルの URL |
| `type` | `schematic` / `board` / `project` / `worksheet` | ファイル種別（自動判定可能だが明示推奨） |
| `name` | ファイル名 | ファイル名（階層回路図のリンクに使われる） |

### Events API（全て未実装）

| イベント | 説明 |
|---|---|
| `kicanvas:click` | ドキュメント内クリック |
| `kicanvas:documentchange` | 表示ドキュメント変更 |
| `kicanvas:error` | ソースファイル読み込みエラー |
| `kicanvas:load` | 全ソースファイル読み込み完了 |
| `kicanvas:loadstart` | ソースファイル読み込み開始 |
| `kicanvas:select` | オブジェクト選択/解除 |

### Deep Linking（未実装）

```html
<kicanvas-embed id="my-schematic" src="my-schematic.kicad_sch" controls="basic">
</kicanvas-embed>
<a href="#my-schematic:Q101">Link to Q101</a>
```

### 注意点: `.kicad_prj` vs `.kicad_pro`

公式ドキュメントの複数ファイル例では `project.kicad_prj` が使われているが、KiCad の現行 project ファイル拡張子は `.kicad_pro`。KiCanvas のソースコード上は `.kicad_pro` も対応しているため、BoardFlow では `.kicad_pro` を使用する。

## BoardFlow への示唆

### BoardFlow で使う属性の組み合わせ

```tsx
// Schematic tab（単一 schematic）
<kicanvas-embed src={schematicUrl} controls="full" controlslist="nodownload" />

// PCB Preview tab（単一 PCB）
<kicanvas-embed src={pcbUrl} controls="full" controlslist="nodownload" />

// Project viewer（複数ファイル）
<kicanvas-embed controls="full" controlslist="nodownload">
  <kicanvas-source src={projectUrl} type="project" name="project.kicad_pro" />
  <kicanvas-source src={schUrl} type="schematic" name="main.kicad_sch" />
  <kicanvas-source src={pcbUrl} type="board" name="board.kicad_pcb" />
</kicanvas-embed>
```

### `name` 属性の重要性

- 階層回路図でサブシートをリンクするために、`name` 属性に元のファイル名を設定する必要がある
- viewer-sources API が `name` フィールドを返す設計は正しい

## 採用/不採用判断

**採用**: `controls="full"` + `controlslist="nodownload"` を基本構成として採用。

理由:
- pan/zoom/select + サイドパネルがフル UI で提供される
- ダウンロードボタンは controlslist で抑制可能
- Events API は未実装だが、BoardFlow MVP では KiCanvas イベントに依存する機能を設計しない方針なので問題なし

## 制約と pitfall

- Events API が全て未実装のため、読み込み完了やエラーをプログラム的に検知できない
  - 対策: ローディングインジケーターは CSS/JS で自前管理し、タイムアウトで fallback 表示
- `theme` / `zoom` 属性は未実装/不完全
- KiCad 5 ファイルは非対応
- KiCad 7 の一部機能（カスタムフォント等）が不完全な可能性

## 未解決の疑問

- `kicanvas:error` / `kicanvas:load` イベントが実装された場合の互換性（MVP では依存しないため影響なし）

## 参照URL

- KiCanvas Embedding API: https://kicanvas.org/embedding/
- KiCanvas Roadmap: https://kicanvas.org/roadmap/
- KiCanvas Development: https://kicanvas.org/development/
