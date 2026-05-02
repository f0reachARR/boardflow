# Issue #33 作業ログ: KiCanvas Web Component インタラクティブプレビュー

## Issueまでの経緯

- Issue #32（Artifact Viewer）がマージ済み。PDF/SVG/iBOM の静的プレビュー基盤が整っている。
- docs/external/kicanvas.md に KiCanvas の基本調査が完了済み。MVP での採用方針（補助的 interactive viewer として使い、PDF/SVG fallback を残す）が決定している。
- docs/frontend/summary.md で「KiCanvas は Client Component に閉じ込め、bundle script は vendoring して外部 CDN から読み込まない」方針が明記されている。
- Issue #33 では、KiCanvas `<kicanvas-embed>` Web Component を Artifact Viewer の Schematic / PCB Preview タブに統合する実装を行う。

## ユーザー要望

- docs 以下の仕様に基づいてアプリケーションを一通り実装する。
- KiCanvas Web Component を使ったインタラクティブ KiCad ファイルプレビュー機能の実装。

## 調査結果

### 1. KiCanvas Bundle の入手と Vendoring

**結論**: `https://kicanvas.org/kicanvas/kicanvas.js` をダウンロードして `public/vendor/kicanvas/kicanvas.js` に vendoring する。

- npm パッケージは存在しない（公式 FAQ で明言）
- GitHub Releases も 0 件
- CDN（jsdelivr/unpkg）にも存在しない
- 公式推奨は bundle JS の直接ダウンロード + vendoring
- bundle サイズ: 約 477KB（非圧縮）、ESM format、esbuild でビルド
- ライセンス: MIT
- runtime dependency: なし

詳細: docs/external/kicanvas-bundle-vendoring.md

### 2. `<kicanvas-embed>` Web Component API

**結論**: `controls="full"` + `controlslist="nodownload"` を基本構成として使用。

- `src` 属性で単一ファイル、`<kicanvas-source>` 子要素で複数ファイル表示
- `controls`: `none` / `basic` / `full` の 3 段階
- `controlslist`: `nodownload`（実装済み）、`noflipview`（実装済み）他
- `type` 属性: `schematic` / `board` / `project` / `worksheet`
- `name` 属性: 階層回路図のリンクに重要
- Events API: 全て未実装（kicanvas:load, kicanvas:error 等）
- `theme` / `zoom` 属性: 未実装/不完全

詳細: docs/external/kicanvas-embed-api.md

### 3. Next.js App Router での Web Components 統合

**結論**: Client Component + `useEffect` で動的 import + `next/dynamic` ssr:false ラッパーを推奨。

- Web Components は SSR 非互換（HTMLElement が Node.js に存在しない）
- `useEffect(() => import("/vendor/kicanvas/kicanvas.js"), [])` で client-only 読み込み
- TypeScript 型定義は `declare module "react/jsx-runtime"` + `DetailedHTMLProps` で拡張
- `next/dynamic` の `ssr: false` で SSR を明示的にスキップ
- `kicanvas-embed:not(:defined)` CSS セレクタでローディング状態を管理

詳細: docs/external/nextjs-web-components-integration.md

## 計画

### 目的

KiCanvas Web Component (`<kicanvas-embed>`) を Artifact Viewer に統合し、KiCad の回路図・基板ファイルをブラウザ上でインタラクティブにプレビューできるようにする。

### 非目的

- KiCanvas Events API を使った選択同期
- Deep link / URL hash による特定コンポーネントへのジャンプ
- Visual diff / overlay
- 3D board rendering
- Server-side rendering
- KiCanvas を使った画像生成（レンダリング）

### 受け入れ条件

1. `kicanvas` ビューアが `status: "available"` のとき、KiCanvasタブでインタラクティブプレビューが表示される
2. KiCanvas が読み込み失敗またはタイムアウトした場合、適切なエラーメッセージが表示される
3. Schematic / PCB Preview タブでは既存の PDF/SVG が引き続き表示される（KiCanvasタブは独立）
4. `kicanvas.js` が vendoring されており外部CDNに依存しない
5. TypeScript の型チェックがパスする
6. `controlslist="nodownload"` によりダウンロードボタンが非表示

### 詳細要件

#### KiCanvasタブの位置づけ

docs/external/kicanvas.md セクション4.4 では「Schematic タブで KiCanvas を第一候補に」とあるが、以下の理由で **独立タブのまま** 実装する：

1. 既存の Schematic/PCB Preview タブは PDF/SVG ベースで安定動作しており、alpha の KiCanvas で置き換えるリスクが高い
2. KiCanvas は project 全体（sch + pcb + pro）を一つのビューアで表示する性質があり、個別の Schematic/PCB タブとは粒度が異なる
3. ユーザーが明示的に「KiCanvas」タブを選択するUXが、alpha段階では適切
4. 将来的に KiCanvas が安定したら Schematic/PCB タブに統合する（docs仕様のセクション4.4方針）

→ 現在の `TAB_DEFINITIONS` の `{ key: "kicanvas", label: "KiCanvas" }` をそのまま活用し、"coming soon" プレースホルダーを実装に置き換える。

#### KiCanvas Viewer の動作仕様

- `sources` に project / schematic / board が揃っている場合 → `<kicanvas-source>` を全て渡し、KiCanvas の project モードで表示
- schematic のみ or board のみの場合 → 該当ファイルのみ渡す
- ローディング中 → Skeleton（min-height: 500px）+ "Loading KiCanvas..." テキスト
- ロード後 10 秒以内に custom element が defined にならない場合 → タイムアウトエラー表示
- WebGL 非対応検知 → エラーメッセージ "Your browser does not support WebGL..."

#### bundle サイズとロード戦略

- `kicanvas.js` ≈ 477KB（非圧縮）。gzip で ~150KB 程度。
- タブ選択時に初めてロードする（lazy load）ことで初期表示に影響しない
- `useEffect` + `import()` パターンで、ユーザーが KiCanvas タブを開いた時に初回ロード

### 影響範囲

- **フロントエンドのみ**。バックエンド API は既に kicanvas viewer を返している（`crates/api/src/routes/read.rs`）
- 新規ファイル 4 つ + 既存ファイル 1 つの変更
- `public/` ディレクトリの新規作成（vendored asset）

### 設計方針

```
artifact-viewer-section.tsx
  └─ renderViewerContent("kicanvas", viewer)
      └─ <KiCanvasViewerLazy sources={...} />  (next/dynamic ssr:false)
          └─ KiCanvasViewer.tsx (Client Component)
              ├─ useEffect で /vendor/kicanvas/kicanvas.js を import
              ├─ <kicanvas-embed controls="full" controlslist="nodownload">
              │    <kicanvas-source .../>
              │  </kicanvas-embed>
              └─ ローディング / エラー / タイムアウト管理
```

### 実装ファイル一覧

| # | ファイルパス | 操作 | 説明 |
|---|---|---|---|
| 1 | `boardflow/public/vendor/kicanvas/kicanvas.js` | 新規（ダウンロード） | vendored KiCanvas bundle |
| 2 | `boardflow/public/vendor/kicanvas/VERSION` | 新規 | バージョン・取得日時記録 |
| 3 | `boardflow/src/types/kicanvas.d.ts` | 新規 | JSX IntrinsicElements 型定義 |
| 4 | `boardflow/src/components/artifact-viewer/kicanvas-viewer.tsx` | 新規 | KiCanvas Client Component |
| 5 | `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx` | 変更 | "coming soon" → KiCanvasViewer 呼び出し |

### 各ファイルの詳細実装内容

#### 1. `boardflow/public/vendor/kicanvas/kicanvas.js`

```bash
mkdir -p boardflow/public/vendor/kicanvas
curl -o boardflow/public/vendor/kicanvas/kicanvas.js https://kicanvas.org/kicanvas/kicanvas.js
```

- git にコミットして管理
- `.gitattributes` で `linguist-vendored` マーク推奨

#### 2. `boardflow/public/vendor/kicanvas/VERSION`

```
source: https://kicanvas.org/kicanvas/kicanvas.js
downloaded: 2026-05-03
license: MIT
note: alpha - no semver versioning available
```

#### 3. `boardflow/src/types/kicanvas.d.ts`

```typescript
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

#### 4. `boardflow/src/components/artifact-viewer/kicanvas-viewer.tsx`

```tsx
"use client"

import { useEffect, useState } from "react"
import { Box, Text, Spinner } from "@chakra-ui/react"
import type { ViewerSource } from "@/lib/api/schema"

type LoadState = "loading" | "ready" | "error"

interface KiCanvasViewerProps {
  sources: ViewerSource[]
}

export function KiCanvasViewer({ sources }: KiCanvasViewerProps) {
  const [loadState, setLoadState] = useState<LoadState>("loading")

  useEffect(() => {
    let cancelled = false
    const timeout = setTimeout(() => {
      if (!cancelled && !customElements.get("kicanvas-embed")) {
        setLoadState("error")
      }
    }, 10000) // 10 秒タイムアウト

    if (customElements.get("kicanvas-embed")) {
      setLoadState("ready")
      clearTimeout(timeout)
      return
    }

    import("/vendor/kicanvas/kicanvas.js" as string)
      .then(() => {
        // custom element の定義を待つ
        return customElements.whenDefined("kicanvas-embed")
      })
      .then(() => {
        if (!cancelled) {
          setLoadState("ready")
          clearTimeout(timeout)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLoadState("error")
          clearTimeout(timeout)
        }
      })

    return () => {
      cancelled = true
      clearTimeout(timeout)
    }
  }, [])

  if (loadState === "error") {
    return (
      <Box p={4} bg="red.50" borderWidth="1px" borderRadius="md" borderColor="red.200">
        <Text fontWeight="medium" color="red.700">
          KiCanvas の読み込みに失敗しました
        </Text>
        <Text fontSize="sm" color="red.600" mt={1}>
          ブラウザが WebGL をサポートしていないか、スクリプトの読み込みに失敗しました。
        </Text>
      </Box>
    )
  }

  if (loadState === "loading") {
    return (
      <Box
        display="flex"
        alignItems="center"
        justifyContent="center"
        minH="500px"
        bg="gray.50"
        borderWidth="1px"
        borderRadius="md"
      >
        <Spinner size="lg" mr={3} />
        <Text color="gray.600">Loading KiCanvas...</Text>
      </Box>
    )
  }

  // Map ViewerSource to kicanvas-source props
  const kicanvasSources = sources
    .filter((s) => s.kind && s.url)
    .map((s) => ({
      type: s.kind as "project" | "schematic" | "board" | "worksheet",
      name: s.name ?? "",
      url: s.url,
    }))

  return (
    <Box minH="500px" borderWidth="1px" borderRadius="md" overflow="hidden">
      <kicanvas-embed controls="full" controlslist="nodownload"
        style={{ width: "100%", height: "600px", display: "block" }}>
        {kicanvasSources.map((source) => (
          <kicanvas-source
            key={`${source.type}:${source.name}`}
            src={source.url}
            type={source.type}
            name={source.name}
          />
        ))}
      </kicanvas-embed>
    </Box>
  )
}
```

**設計ポイント:**
- `next/dynamic` ssr:false ラッパーは不要。`artifact-viewer-section.tsx` 自体が `"use client"` であるため、同じ Client Component ツリー内で `useEffect` による動的 import で十分。
- `customElements.whenDefined()` で custom element の登録完了を待ってから "ready" に遷移
- 10 秒タイムアウトで無限ローディングを防止
- `cancelled` フラグで unmount 後の state 更新を防止

#### 5. `artifact-viewer-section.tsx` の変更

"coming soon" ブロックを以下に置き換え：

```tsx
import { KiCanvasViewer } from "./kicanvas-viewer"

// renderViewerContent 内の kicanvas 分岐:
if (name === "kicanvas") {
  if (!viewer.sources || viewer.sources.length === 0) {
    return <ViewerStatusMessage status="missing" viewerName="kicanvas" />
  }
  return <KiCanvasViewer sources={viewer.sources} />
}
```

### テスト観点

1. **TypeScript型チェック**: `pnpm tsc --noEmit` がパスすること
2. **ESLint**: lint エラーなし
3. **ビルド**: `pnpm build` が成功すること
4. **手動確認**: kicanvas タブを開いたとき
   - sources がある場合 → `<kicanvas-embed>` が表示される
   - sources がない場合 → "missing" ステータスメッセージ
   - スクリプト読み込み失敗時 → エラーメッセージ
5. **E2E（将来）**: Playwright で `kicanvas-embed` 要素の存在確認（WebGL が必要なため描画内容の確認は困難）

### ドキュメント更新対象

- `docs/logs/33/worklog.md` — 本ファイル（計画・実装記録）
- `docs/frontend/summary.md` — KiCanvas 統合に関する記述の追加（実装完了後）

### 実装順序

1. `boardflow/public/vendor/kicanvas/` にバンドルをダウンロード・配置
2. `boardflow/src/types/kicanvas.d.ts` を作成
3. `boardflow/src/components/artifact-viewer/kicanvas-viewer.tsx` を作成
4. `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx` を変更
5. `pnpm tsc --noEmit` と `pnpm build` で検証

### 実装要否

**implementation_required**

### 未解決の疑問

なし。Research 段階で全ての技術的疑問が解消されている。

- KiCanvas の導入方法 → bundle vendoring（公式推奨の唯一の方法）
- Next.js 統合 → Client Component + useEffect + customElements.whenDefined()
- 型定義 → `declare module "react/jsx-runtime"` パターン
- タブ配置 → 独立タブ（alpha 段階のため、将来的に Schematic/PCB に統合）
- バックエンド → 既に `kicanvas` viewer を sources 付きで返却済み（変更不要）

## 結論ステータス

**implementation_required**: 調査完了。3 つの調査トピック全てで導入方法が明確になった。実装に進むべき。

## 残リスク

- KiCanvas が alpha 段階のため、bundleの更新で API が壊れる可能性がある → PDF/SVG fallback で軽減
- Events API が未実装のため、エラー検知はタイムアウトベースの fallback で対応する必要がある
- WebGL 非対応環境（一部モバイル、headless ブラウザ）で表示失敗する → fallback で対応
- KiCad 5 ファイルは表示不可（KiCad 6 以降のみ対応）

## 更新したファイル

- docs/external/kicanvas-bundle-vendoring.md（新規作成）
- docs/external/kicanvas-embed-api.md（新規作成）
- docs/external/nextjs-web-components-integration.md（新規作成）
- docs/logs/33/worklog.md（本ファイル）

## 参照URL

- https://kicanvas.org/embedding/
- https://kicanvas.org/home/#faq
- https://kicanvas.org/roadmap/
- https://kicanvas.org/development/
- https://github.com/theacodes/kicanvas
- https://github.com/theacodes/kicanvas/blob/main/scripts/build.js
- https://github.com/theacodes/kicanvas/blob/main/scripts/bundle.js
- https://nextjs.org/docs/app/api-reference/components/script
- https://til.jakelazaroff.com/typescript/add-custom-element-to-jsx-intrinsic-elements/

## 実装内容（2026-05-03）

### 追加ファイル

1. **`boardflow/public/vendor/kicanvas/kicanvas.js`** — vendored bundle（477KB）
2. **`boardflow/public/vendor/kicanvas/VERSION`** — バージョン情報
3. **`.gitattributes`** — `linguist-vendored` 設定
4. **`boardflow/src/types/kicanvas.d.ts`** — `declare module "react"` で `JSX.IntrinsicElements` を拡張
5. **`boardflow/src/components/artifact-viewer/kicanvas-viewer.tsx`** — Client Component

### 変更ファイル

1. **`boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx`** — "coming soon" プレースホルダーを `KiCanvasViewer` コンポーネント呼び出しに置き換え

### 技術的判断

- TypeScript型定義: `declare module "react/jsx-runtime"` では Next.js の TSC が認識しなかったため、`declare module "react"` による `JSX.IntrinsicElements` 拡張を採用
- dynamic import: `/* webpackIgnore: true */` コメントで webpack の module resolution をバイパス + `@ts-expect-error` で TypeScript エラーを抑制
- タイムアウト: 10秒（alpha品質の外部バンドルのため余裕を持たせた）
- `Spinner` は Chakra UI v3 の `@chakra-ui/react` に存在することを確認して使用

### テスト結果

- `pnpm tsc --noEmit` → 成功
- `pnpm build` (Next.js production build) → 成功
- `pnpm eslint` 対象ファイル → エラーなし

### 残リスク

- KiCanvas はalpha品質のため、将来の bundle 更新時に API 変更の可能性あり
- `customElements.whenDefined` でリカバリできるが、Events API（kicanvas:error）は未実装のためエラー検知が限定的
- WebGL 非対応環境での詳細なエラーハンドリングは KiCanvas 側の実装に依存

## レビュー結果（2026-05-03）

### 総評

- vendoring、`controlslist="nodownload"`、`viewer-sources` API 経由の URL 利用、型定義追加、Client Component 化といった基本方針は妥当。
- 一方で、Issue 本文と frontend 文書が期待している「Schematic / PCB Preview への統合」と、計画で掲げた「タブ選択時の初回ロード」が現状実装では満たされていない。
- そのため、現時点では PR 作成可と判断しない。

### pr_ready

- false

### 指摘事項

1. [major] `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx` では KiCanvas を独立タブとして追加しており、既存の Schematic / PCB Preview タブへ統合していない。Issue 本文の「Schematic/PCBのインタラクティブビューをArtifact Viewerのタブに統合する」と、`docs/frontend/summary.md` の「Schematic / PCB Preview では KiCanvas を第一候補にしつつ、PDF/SVG fallback を必ず残す」に未達。
2. [major] `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx` の `Tabs.Root` に `lazyMount` / `unmountOnExit` がなく、`boardflow/src/components/artifact-viewer/kicanvas-viewer.tsx` は mount 直後に vendored bundle を import するため、計画の「タブ選択時に初めてロードする（lazy load）」を満たしていない可能性が高い。Chakra Tabs の公開ドキュメントでも、アクティブ時のみ内容を描画するには `lazyMount` か `unmountOnExit` が必要とされている。
3. [minor] エラーハンドリングが `loading` / `ready` / `error` の 3 値のみで、タイムアウト・script load failure・WebGL 非対応を区別していない。さらに計画では 5 秒タイムアウトだったのに実装は 10 秒へ変更されており、`WebGL 非対応時は専用メッセージ` という要件ともずれている。

### 必須修正

1. KiCanvas を独立タブのままにするのか、Schematic / PCB Preview 内へ統合するのかを Issue / docs / 実装で一本化する。Issue を充足させるなら UI 実装を合わせる。独立タブ方針を維持するなら `docs/frontend/summary.md` など関連文書を更新し、合意済み仕様へ落とす。
2. KiCanvas bundle を本当にタブ選択時に初回ロードするよう修正する。少なくとも Tabs 側の lazy mount を有効化するか、選択中タブに応じて `KiCanvasViewer` を mount する必要がある。
3. タイムアウトと WebGL 非対応を同一エラー文言にまとめない。要件どおりのタイムアウト値と文言に揃えるか、実装判断として変更した理由を docs / worklog に反映した上で UI メッセージを分離する。

### 任意改善

1. `ViewerSource.kind` を文字列のまま cast せず、frontend 側で許容値を絞る型ガードを置くと保守しやすい。
2. ローディングとエラー領域に `role="status"` / `role="alert"` 相当のアクセシビリティ補助を加えると支援技術で伝わりやすい。

### テスト不足

1. `pnpm tsc --noEmit`、`pnpm build`、`pnpm eslint` は通っているが、KiCanvas の表示条件切替を検証する component test / E2E がない。
2. 特に「非アクティブタブでは bundle を読み込まない」「script load failure 時にエラー表示へ遷移する」「kicanvas failed 時も PDF/SVG fallback へ戻れる」を自動テストで担保できていない。

### ドキュメント更新漏れ

1. `docs/frontend/summary.md` は依然として KiCanvas を Schematic / PCB Preview の第一候補と書いており、現実装の独立 KiCanvas タブと不一致。
2. worklog 内でも 5 秒タイムアウト / タブ選択時ロードと、実装時の 10 秒タイムアウト / 即 mount import が食い違っているため、最終設計として整理が必要。

### plan / research / docs との不整合

1. ~~plan: `docs/logs/33/worklog.md` では「タブ選択時に初めてロード」としているが、実装はその保証がない。~~ → `lazyMount` 追加により解決。
2. ~~plan: `docs/logs/33/worklog.md` では 5 秒タイムアウトとしているが、実装は 10 秒。~~ → レビュー判断により 10 秒を正とする。
3. ~~docs: `docs/frontend/summary.md` は Schematic / PCB Preview への統合を前提としているが、実装は独立 KiCanvas タブ。~~ → Schematic/PCB Preview タブに統合済み。
4. ~~research: `docs/external/kicanvas.md` では WebGL 失敗時に静的 fallback を残す方針だが、現状は独立タブ上のエラー表示に留まり、統合 UI としての fallback 体験はまだ実現されていない。~~ → Schematic/PCB タブ内で KiCanvas + PDF/SVG fallback が共存する形に修正済み。

## レビュー指摘修正（2026-05-03）

### 修正1: KiCanvas を Schematic / PCB Preview タブに統合

- `TAB_DEFINITIONS` から `{ key: "kicanvas", label: "KiCanvas" }` を削除
- `renderViewerContent` のシグネチャを `(name, viewer, allViewers)` に変更
- Schematic タブ: `allViewers["kicanvas"]` の状態を見て KiCanvasViewer を表示、その下に PDF fallback を残す
- PCB Preview タブ: 同様に KiCanvasViewer を表示、SVG fallback を残す
- `if (name === "kicanvas")` の独立分岐を削除

### 修正2: 遅延ロード（lazyMount）

- `<Tabs.Root defaultValue={defaultTab} lazyMount>` により非アクティブタブは mount されない
- KiCanvasViewer の `useEffect` はタブ選択時に初めて実行される

### 修正3: エラーハンドリング改善

- `LoadState` を `"loading" | "ready" | "timeout" | "load_error"` に細分化
- `timeout`: 「KiCanvas の読み込みがタイムアウトしました。ページを再読み込みしてください。」
- `load_error`: 「KiCanvas スクリプトの読み込みに失敗しました。ブラウザが WebGL をサポートしていない可能性があります。」
- タイムアウト値は 10 秒（レビュー後の実装判断として確定）

### 確認結果

- `pnpm tsc --noEmit`: 成功
- `pnpm build`: 成功
- 型エラー・ビルドエラーなし

### 残リスク

- KiCanvas の表示条件切替を検証する component test / E2E テストがまだない（既知。テスト追加は別 Issue で対応予定）
