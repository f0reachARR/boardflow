# Chakra UI v3 + Next.js App Router セットアップ

## 要約

Chakra UI v3 (現在 v3.35.0) は Next.js 15/16 の App Router に対応している。Emotion をランタイムで使用し、`@chakra-ui/react` + `@emotion/react` のみでインストール可能（v2 で必要だった `framer-motion` や `@chakra-ui/next-js` は不要）。Provider は CLI が生成する snippets の `components/ui/provider` を使う。

## 確認した情報

### インストール手順

```bash
# 1. パッケージインストール
npm i @chakra-ui/react @emotion/react

# 2. Snippets 追加（pre-built コンポーネント群）
npx @chakra-ui/cli snippet add

# 3. tsconfig.json 更新
```

### tsconfig.json 必要設定

```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "skipLibCheck": true,
    "paths": {
      "@/*": ["./src/*"]
    }
  }
}
```

### Provider 設定 (app/layout.tsx)

```tsx
import { Provider } from "@/components/ui/provider"

export default function RootLayout(props: { children: React.ReactNode }) {
  const { children } = props
  return (
    <html suppressHydrationWarning>
      <body>
        <Provider>{children}</Provider>
      </body>
    </html>
  )
}
```

Provider は以下を compose している:
- `ChakraProvider` from `@chakra-ui/react` (スタイリングシステム)
- `ThemeProvider` from `next-themes` (カラーモード)

### next.config.mjs 最適化

```js
export default {
  experimental: {
    optimizePackageImports: ["@chakra-ui/react"],
  },
}
```

### Server Components との互換性

- Chakra UI v3 のコンポーネントは Client Components でのみ使用可能
- Server Components で使うには `'use client'` を付ける必要がある
- ただし v3 では v2 のような `@chakra-ui/next-js` の `CacheProvider` は不要になった
- Provider 自体が Client Component boundary を作るので、layout.tsx に配置すれば子コンポーネントは自然に Client Component として扱える

## BoardFlow への示唆

- `docs/frontend/summary.md` のアーキテクチャ方針「Server Components 優先、必要な部分だけ Client Components」と Chakra UI の「Client Component のみ」制約は矛盾するが、実用上は問題ない
- 一覧画面などデータフェッチは Server Component で行い、UI 表示部分を Client Component に分離するパターンが推奨される
- Snippets システムにより、Button, Dialog, Tooltip 等の頻出コンポーネントが `components/ui/` に自動生成される

## BoardFlow 実装との差分

BoardFlow の実装では以下の点で Chakra UI 公式推奨構成から簡略化している:

- **Snippets 未使用**: `npx @chakra-ui/cli snippet add` による UI コンポーネント生成は行っていない。必要に応じて段階的に追加する方針。
- **next-themes 未使用**: 公式 Provider は `next-themes` の ThemeProvider を含むが、BoardFlow では `ChakraProvider` のみの最小構成を採用。ダークモード対応は MVP スコープ外。
- **Provider 最小構成**: `boardflow/src/components/ui/provider.tsx` は `ChakraProvider` + `defaultSystem` のみ。カスタムテーマは未設定。

## 採用判断

**採用**: `docs/technology.md` で Chakra UI は決定済み。v3 が最新安定版であり、Next.js App Router 対応も公式サポートされている。

## 制約と pitfall

1. **Turbopack hydration エラー**: Turbopack 使用時に hydration エラーが発生する既知の問題あり。Next.js 15.5 ではデフォルトが webpack なので `--turbo` を指定しなければ問題なし。
2. **`suppressHydrationWarning`**: `<html>` タグに必須。`next-themes` が原因。
3. **Node.js 20.x 以上が必須**
4. **Emotion ランタイム**: 現在はランタイム CSS-in-JS。将来的にゼロランタイム (Panda CSS inspired) への移行が計画されているが、現時点では Emotion 依存。
5. **Turbopack 非互換**: Chakra UI v3 は Turbopack と互換性がないため、`next dev --turbo` は使用不可。デフォルト (webpack) のままで開発する。

## 未解決の疑問

- Chakra UI v3 のカスタムテーマ設定方法（BoardFlow 固有のブランドカラー等）
- Snippets の部分追加（全部ではなく必要なものだけ追加する方法）

## 参照URL

- https://chakra-ui.com/docs/get-started/installation
- https://chakra-ui.com/docs/get-started/frameworks/next-app
- https://v2.chakra-ui.com/getting-started/nextjs-app-guide (v2参考、v3では不要な設定の確認用)
