# Next.js App Router で Web Components を使うベストプラクティス

## 要約

Next.js App Router で Web Components（custom elements）を使うには、Client Component に閉じ込め、`useEffect` 内で動的に script をロードする。TypeScript では `declare module "react/jsx-runtime"` で JSX.IntrinsicElements を拡張する。`next/script` の `strategy="afterInteractive"` も有効だが、vendored script の場合は `useEffect` + dynamic import の方が制御しやすい。

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

### パターン 3: `next/dynamic` with `ssr: false`

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

### TypeScript JSX.IntrinsicElements 型定義

React 19 以降は custom elements をネイティブサポートするが、TypeScript の型チェックには明示的な定義が必要。

#### 推奨パターン（React + Next.js）

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

declare module "react/jsx-runtime" {
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

#### 注意点

- `declare namespace JSX` ではなく `declare module "react/jsx-runtime"` を使う（React 18+ / Next.js App Router の jsx transform に対応）
- `@types/react` が JSX.IntrinsicElements を定義しているため、単純な namespace 再宣言は機能しない
- `DetailedHTMLProps` を使うことで `key`、`ref`、`className` などの React 標準 props も通る

### `next/script` の strategy オプション

| strategy | 動作 | KiCanvas での適性 |
|---|---|---|
| `beforeInteractive` | hydration 前に読み込み | 不要。KiCanvas は後で良い |
| `afterInteractive` | ページ interactive 後に読み込み（デフォルト） | **推奨** |
| `lazyOnload` | ブラウザ idle 時に読み込み | タブ切り替え時に遅延が目立つ可能性 |
| `worker` | Web Worker で実行（experimental） | KiCanvas は DOM/Canvas 必須なので不可 |

## BoardFlow への示唆

### 推奨アーキテクチャ

```
Server Component (page.tsx)
  └─ fetch viewer-sources API（サーバーサイド）
  └─ viewer status / URL を props で渡す
      └─ next/dynamic で ssr: false ラップ
          └─ Client Component (KiCanvasViewer)
              ├─ useEffect で kicanvas.js を import
              ├─ <kicanvas-embed> をレンダリング
              └─ loading / error / fallback 表示
```

### 具体的なファイル構成案

```
boardflow/
  public/
    vendor/
      kicanvas/
        kicanvas.js          # vendored bundle
        VERSION               # 取得日時と commit hash
  src/
    types/
      kicanvas.d.ts           # JSX IntrinsicElements 型定義
    components/
      KiCanvasViewer.tsx      # Client Component
      KiCanvasViewerLazy.tsx  # next/dynamic ラッパー（ssr: false）
```

### Script 読み込みの重複防止

KiCanvas の custom element 定義は一度だけ行えばよい。`next/script` は同一 `src` の重複挿入を防ぐ機構がある。`useEffect` パターンの場合は、以下のガードを入れる:

```tsx
useEffect(() => {
  if (!customElements.get("kicanvas-embed")) {
    import("/vendor/kicanvas/kicanvas.js");
  }
}, []);
```

## 採用/不採用判断

**採用**: パターン 1（`useEffect` + import）+ `next/dynamic` ssr:false ラッパーの組み合わせを推奨。

理由:
- vendored file の読み込みに最も適している
- SSR 回避が明示的で安全
- `next/script` は追加で使ってもよいが、必須ではない
- TypeScript 型定義は `declare module "react/jsx-runtime"` パターンで確立

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
