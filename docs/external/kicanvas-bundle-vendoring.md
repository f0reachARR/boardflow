# KiCanvas Bundle の入手と Vendoring 方法

## 要約

KiCanvas は npm パッケージとして公開されておらず、GitHub Releases も存在しない。公式の導入方法は kicanvas.org から bundled JS を直接ダウンロードして vendoring する方式のみ。BoardFlow では `public/vendor/kicanvas/kicanvas.js` に配置して `<script type="module">` で読み込む。

## 確認した情報

### npm パッケージ

- npmjs.com に `kicanvas` パッケージは存在しない（2026-05-03 確認）。
- 公式 FAQ に「KiCanvas's developer-facing APIs for embedding and parsing are not yet ready. I don't want to publish it only to immediately break users as I rapidly iterate and change things.」と明記されている。
- npm 公開は当面予定なし。

### GitHub Releases

- https://github.com/theacodes/kicanvas/releases に release は 0 件（2026-05-03 確認）。
- `package.json` の `version` は `"0.0.0"` のまま。

### CDN 配信

- jsdelivr / unpkg には KiCanvas は存在しない（npm パッケージがないため）。
- kicanvas.org 自体は GitHub Pages で配信されており、`https://kicanvas.org/kicanvas/kicanvas.js` は `Access-Control-Allow-Origin: *` 付きで公開されているが、これは kicanvas.org のサイト用であり、外部からの CDN 利用を公式にサポートしているわけではない。

### 公式推奨の導入方法

公式 Embedding ドキュメント（https://kicanvas.org/embedding/）に明記:

> During alpha, the best way to install KiCanvas is to [download the bundled kicanvas.js](https://kicanvas.org/kicanvas/kicanvas.js), copy it into your project, and include it with a script tag:
>
> ```html
> <script type="module" src="/kicanvas.js"></script>
> ```

### Bundle のビルド構成

- エントリポイント: `src/index.ts`
- ビルドツール: esbuild
- 出力: `build/kicanvas.js`（ESM format, target es2022, minified, sourcemap なし）
- runtime dependency: なし（すべて bundle に含まれる）
- bundle サイズ: 約 477KB（非圧縮）、gzip 後は推定 120-150KB 程度

### ソースからビルドする場合

```bash
git clone https://github.com/theacodes/kicanvas.git
cd kicanvas
npm install
npm run build
# build/kicanvas.js が生成される
```

## BoardFlow への示唆

### 推奨 vendoring 手順

1. `https://kicanvas.org/kicanvas/kicanvas.js` をダウンロード
2. `boardflow/public/vendor/kicanvas/kicanvas.js` に配置
3. git にコミットして管理
4. 更新時はダウンロードし直して差し替え

### 代替案: git submodule + ビルド

- theacodes/kicanvas を submodule として追加し、CI でビルドして `public/vendor/` にコピーする方式も可能
- ただし alpha 段階で頻繁に壊れる可能性があるため、MVP では安定版の bundle を vendoring して固定する方が安全

## 採用/不採用判断

**採用**: 公式推奨の「bundle ダウンロード + vendoring」方式を採用する。

理由:
- 公式が唯一推奨している方法
- npm / CDN は利用不可
- private artifact を扱う画面で外部 CDN からのスクリプト読み込みを避けるセキュリティ方針とも合致
- bundle サイズ（~477KB）は許容範囲

## 制約と pitfall

- KiCanvas のバージョン管理が困難（semver なし、release なし）
  - 対策: vendoring 時に取得日時と git commit hash をコメントまたは別ファイルで記録
- bundle 更新時に API 破壊変更がある可能性
  - 対策: Playwright smoke test で `<kicanvas-embed>` の描画を検証
- alpha 段階のため、一部機能が未実装・不安定
  - 対策: PDF/SVG fallback を必ず残す

## 未解決の疑問

- なし。導入方法は公式ドキュメントで明確。

## 参照URL

- KiCanvas Embedding Installation: https://kicanvas.org/embedding/#installation
- KiCanvas FAQ (Why not on NPM): https://kicanvas.org/home/#faq
- KiCanvas GitHub: https://github.com/theacodes/kicanvas
- KiCanvas build script: https://github.com/theacodes/kicanvas/blob/main/scripts/build.js
- KiCanvas bundle script: https://github.com/theacodes/kicanvas/blob/main/scripts/bundle.js
