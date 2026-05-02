# Next.js App Router で Web Components を使うベストプラクティス

## 要約

Next.js App Router で Web Components（custom elements）を使うには、Client Component に閉じ込め、`useEffect` 内で動的に script をロードする。TypeScript では `declare module "react"` で JSX.IntrinsicElements を拡張する。`next/script` の `strategy="afterInteractive"` も有効だが、vendored script の場合は `useEffect` + dynamic import の方が制御しやすい。

> **BoardFlow での採用**: `declare module "react"` + Client Component 直接 import + `Tabs.Root lazyMount` による遅延マウントを採用。`next/dynamic` + `ssr: false` は不使用。詳細は末尾「BoardFlow での採用」セクション参照。

## 確認した情報

### 問題の背景

1. **SSR 非互換**: Web Components は `HTMLElement` を前提としており、Node.js 環境（Server Components）では `ReferenceError: HTMLElement is not defined` になる
2. **TypeScript エラー**: `Property 'kicanvas-embed' does not exist on type 'JSX.IntrinsicElements'`
3. **スクリプト読み込みタイミング**: custom element の定義前にレンダリングすると、ブラウザは未知のタグとして扱う

### パターン 1: `useEffect` + dynamic import（推奨）

```tsx
"use client";

import { useEffect, useRef } from "react";

export function KiCanvasViewer({ src }: { src: string }) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // ブラウザでのみ実行される
    import("/vendor/kicanvas/kicanvas.js");
  }, []);

  return (
    <div ref={containerRef}>
      <kicanvas-embed src={src} controls="full" controlslist="nodownload" />
    </div>
  );
}
```

メリット:
- SSR でスクリプト読み込みが発生しない
- コンポーネントのライフサイクルに紐づく
- vendored file でも external URL でも同じパターン

### パターン 2: `next/script` with `strategy`

```tsx
"use client";

import Script from "next/script";

export function KiCanvasViewer({ src }: { src: string }) {
  return (
    <>
      <Script
        src="/vendor/kicanvas/kicanvas.js"
        strategy="afterInteractive"
        type="module"
      />
      <kicanvas-embed src={src} controls="full" controlslist="nodownload" />
    </>
  );
}
```

メリット:
- Next.js が読み込み最適化を行う
- `onLoad` / `onError` コールバックが使える

注意:
- `next/script` は `type="module"` をそのまま forward する
- KiCanvas bundle は ESM format なので `type="module"` が必須

### パターン 3: `next/dynamic` with `ssr: false`（代替案）

```tsx
import dynamic from "next/dynamic";

const KiCanvasViewer = dynamic(() => import("./KiCanvasViewer"), {
  ssr: false,
  loading: () => <div>Loading viewer...</div>,
});
```

メリット:
- SSR を完全にスキップ
- loading 状態を declarative に定義できる

注意:
- コンポーネントが既に Client Component ツリー内にある場合は不要（Client Component は SSR しないため）
- Chakra UI `Tabs.Root lazyMount` など親コンポーネントで遅延マウントできる場合は、追加のラッパーなしで同等の効果が得られる
- BoardFlow では不採用（後述の「BoardFlow での採用」セクション参照）

### TypeScript JSX.IntrinsicElements 型定義

React 19 以降は custom elements をネイティブサポートするが、TypeScript の型チェックには明示的な定義が必要。

#### パターン A: `declare module "react"`（BoardFlow で採用）

```typescript
// types/kicanvas.d.ts
import type { DetailedHTMLProps, HTMLAttributes } from "react";

interface KiCanvasEmbedAttributes {
  src?: string;
  controls?: "none" | "basic" | "full";
  controlslist?: string;
  theme?: string;
  zoom?: string;
}

interface KiCanvasSourceAttributes {
  src?: string;
  type?: "schematic" | "board" | "project" | "worksheet";
  name?: string;
}

declare module "react" {
  namespace JSX {
    interface IntrinsicElements {
      "kicanvas-embed": DetailedHTMLProps<
        HTMLAttributes<HTMLElement> & KiCanvasEmbedAttributes,
        HTMLElement
      >;
      "kicanvas-source": DetailedHTMLProps<
        HTMLAttributes<HTMLElement> & KiCanvasSourceAttributes,
        HTMLElement
      >;
    }
  }
}
```

この `types/kicanvas.d.ts` を `tsconfig.json` の `include` に含めるか、Next.js のプロジェクトルートに置くことで型が認識される。

#### パターン B: `declare module "react/jsx-runtime"`（代替案）

```typescript
declare module "react/jsx-runtime" {
  namespace JSX {
    interface IntrinsicElements {
      "kicanvas-embed": DetailedHTMLProps<
        HTMLAttributes<HTMLElement> & KiCanvasEmbedAttributes,
        HTMLElement
      >;
    }
  }
}
```

React 18+ の jsx transform に対応する形式。`@types/react` のバージョンや `tsconfig.json` の `jsx` 設定によってはこちらが必要になるケースもあるが、BoardFlow では `declare module "react"` で問題なく型が解決されたためパターン A を採用した。

#### 注意点

- `declare namespace JSX` は単独では機能しない（`@types/react` が JSX.IntrinsicElements を定義しているため、module augmentation が必要）
- `DetailedHTMLProps` を使うことで `key`、`ref`、`className` などの React 標準 props も通る
- どちらのパターンでも `tsconfig.json` の `include` にファイルが含まれている必要がある

### `next/script` の strategy オプション

| strategy | 動作 | KiCanvas での適性 |
|---|---|---|
| `beforeInteractive` | hydration 前に読み込み | 不要。KiCanvas は後で良い |
| `afterInteractive` | ページ interactive 後に読み込み（デフォルト） | **推奨** |
| `lazyOnload` | ブラウザ idle 時に読み込み | タブ切り替え時に遅延が目立つ可能性 |
| `worker` | Web Worker で実行（experimental） | KiCanvas は DOM/Canvas 必須なので不可 |

## BoardFlow での採用

### 採用した手法

**パターン 1（`useEffect` + import）を Client Component 内で直接使用。`next/dynamic` ラッパーは不使用。**

理由:
- `artifact-viewer-section.tsx` は `"use client"` の Client Component ツリー内にあり、SSR は発生しない
- `Tabs.Root lazyMount` により、KiCanvas タブが選択されるまでコンポーネント自体がマウントされない → `next/dynamic` の `ssr: false` と同等の遅延効果が得られる
- ラッパーコンポーネントが不要になりファイル数が減る

### 型定義

`declare module "react"` パターンを採用（パターン A）。`declare module "react/jsx-runtime"` ではない。
Next.js 15 + `@types/react` 19 環境で問題なく型解決されることを確認済み。

### 実際のアーキテクチャ

```
Server Component (page.tsx)
  └─ fetch viewer-sources API（サーバーサイド）
      └─ viewer status / URL を props で渡す
          └─ artifact-viewer-section.tsx ("use client")
              └─ Tabs.Root lazyMount
                  └─ KiCanvasViewer (Client Component, 直接 import)
                      ├─ useEffect で /vendor/kicanvas/kicanvas.js を import
                      ├─ customElements.whenDefined で定義待ち
                      ├─ <kicanvas-embed> をレンダリング
                      └─ loading / error / timeout 表示
```

### 具体的なファイル構成

```
boardflow/
  public/
    vendor/
      kicanvas/
        kicanvas.js          # vendored bundle
        VERSION               # 取得日時と commit hash
  src/
    types/
      kicanvas.d.ts           # JSX IntrinsicElements 型定義 (declare module "react")
    components/
      artifact-viewer/
        kicanvas-viewer.tsx   # Client Component（直接 import される）
```

### 不採用とした手法

| 手法 | 理由 |
|---|---|
| `next/dynamic` + `ssr: false` ラッパー | Client Component ツリー + `lazyMount` で十分。追加のラッパーファイルは冗長 |
| `declare module "react/jsx-runtime"` | `declare module "react"` で型解決できたため不要 |
| `next/script` | vendored script の動的 import で制御できており、`next/script` の最適化は不要 |

### Script 読み込みの重複防止

KiCanvas の custom element 定義は一度だけ行えばよい。`useEffect` パターンで以下のガードを入れている:

```tsx
useEffect(() => {
  if (customElements.get("kicanvas-embed")) {
    setLoadState("ready")
    return
  }
  import(/* webpackIgnore: true */ "/vendor/kicanvas/kicanvas.js")
    .then(() => customElements.whenDefined("kicanvas-embed"))
    .then(() => setLoadState("ready"))
    .catch(() => setLoadState("load_error"))
}, [])
```

## 一般的な採用判断ガイドライン

以下は一般的な Next.js プロジェクトでの選択指針。BoardFlow での具体的な判断は上記「BoardFlow での採用」セクションを参照。

- **Client Component ツリー内 + UI ライブラリの遅延マウント機能あり** → パターン 1（`useEffect` + import）のみで十分
- **Server Component から直接使いたい** → パターン 3（`next/dynamic` + `ssr: false`）が必要
- **外部 CDN からロード** → パターン 2（`next/script`）が最適化の面で有利
- **vendored bundle** → パターン 1 が最も制御しやすい

TypeScript 型定義は `declare module "react"` をまず試し、環境次第で `"react/jsx-runtime"` に切り替える。

## 制約と pitfall

- KiCanvas は WebGL を使うため、headless ブラウザやGPU のない環境では表示失敗する
  - Playwright テストでは WebGL が使える Chrome を使用すること
- `useEffect` での import は初回レンダリング時に custom element 未定義の瞬間がある
  - 対策: CSS で `kicanvas-embed:not(:defined)` に min-height とローディング表示を設定
- KiCanvas bundle は ESM のため、`<script>` タグには `type="module"` が必須
- Web Components の Shadow DOM により、外部からの CSS スタイリングは制限される

## 未解決の疑問

- なし。パターンは確立されている。

## 参照URL

- Next.js Script Component: https://nextjs.org/docs/app/api-reference/components/script
- Next.js dynamic import: https://nextjs.org/docs/app/building-your-application/optimizing/lazy-loading
- React 19 Custom Elements Support: https://custom-elements-everywhere.com/libraries/react/results/results.html
- TypeScript JSX IntrinsicElements for custom elements: https://til.jakelazaroff.com/typescript/add-custom-element-to-jsx-intrinsic-elements/
- DEV Community - Web Components with Next.js: https://dev.to/swyx/how-to-use-web-components-with-next-js-and-typescript-4hhc
