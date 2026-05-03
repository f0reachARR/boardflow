# TanStack Query v5 + Next.js 15 App Router 統合パターン

Issue: #64

## 要約

TanStack Query v5 を Next.js 15 App Router + openapi-fetch と統合するための調査結果。公式ドキュメント (tanstack.com/query/v5/docs/framework/react/guides/advanced-ssr) で推奨される prefetch + dehydration パターンと、openapi-fetch エコシステムの公式ラッパー `openapi-react-query` の活用を中心にまとめる。

## 確認した情報

### 1. QueryClient セットアップパターン（公式推奨）

TanStack Query v5 の Advanced SSR ガイドでは、以下のパターンが推奨されている。

#### `get-query-client.ts` — isServer による分岐

```ts
// app/get-query-client.ts
import {
  isServer,
  QueryClient,
  defaultShouldDehydrateQuery,
} from '@tanstack/react-query'

function makeQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 60 * 1000, // 1分
      },
      dehydrate: {
        // pending クエリも dehydration に含める（streaming 対応）
        shouldDehydrateQuery: (query) =>
          defaultShouldDehydrateQuery(query) ||
          query.state.status === 'pending',
      },
    },
  })
}

let browserQueryClient: QueryClient | undefined = undefined

export function getQueryClient() {
  if (isServer) {
    // サーバー: リクエストごとに新規作成
    return makeQueryClient()
  } else {
    // ブラウザ: シングルトン（Suspense での再生成を防ぐ）
    if (!browserQueryClient) browserQueryClient = makeQueryClient()
    return browserQueryClient
  }
}
```

**重要ポイント:**
- サーバー側: **リクエストごとに新しい QueryClient** を作成（リクエスト間でデータを共有しない）
- ブラウザ側: **シングルトン**（React の Suspense で再レンダリングされても同じインスタンスを使う）
- `useState` で QueryClient を初期化するのは避ける（Suspense boundary が上位にない場合、React が初回レンダリングで破棄する）

#### `providers.tsx` — QueryClientProvider（Client Component）

```tsx
// app/providers.tsx
'use client'

import { QueryClientProvider } from '@tanstack/react-query'
import { getQueryClient } from './get-query-client'

export default function Providers({ children }: { children: React.ReactNode }) {
  const queryClient = getQueryClient()
  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
}
```

#### `layout.tsx` — Provider の組み込み

```tsx
// app/layout.tsx
import Providers from './providers'

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="ja">
      <body>
        <Providers>{children}</Providers>
      </body>
    </html>
  )
}
```

### 2. Prefetch + Dehydration パターン（公式推奨）

Server Component で prefetch → Client Component で useQuery のパターン。

#### Server Component（page.tsx）

```tsx
// app/posts/page.tsx
import { dehydrate, HydrationBoundary } from '@tanstack/react-query'
import { getQueryClient } from './get-query-client'
import Posts from './posts'

export default async function PostsPage() {
  const queryClient = getQueryClient()

  await queryClient.prefetchQuery({
    queryKey: ['posts'],
    queryFn: getPosts,
  })

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Posts />
    </HydrationBoundary>
  )
}
```

#### Client Component

```tsx
// app/posts/posts.tsx
'use client'

export default function Posts() {
  const { data } = useQuery({
    queryKey: ['posts'],
    queryFn: () => getPosts(),
  })
  // data は prefetch 済みなので即座に利用可能
}
```

**重要ポイント:**
- `<HydrationBoundary>` はルートごとに必要（省略不可）
- ネストされた Server Component でもそれぞれ `<HydrationBoundary>` を使える
- `prefetchQuery` → `useQuery` の場合、クライアントで suspend しない（prefetch 漏れ時はクライアント fetch にフォールバック）
- `prefetchQuery` (await なし) → `useSuspenseQuery` で streaming パターンも可能（v5.40.0+）

### 3. Streaming パターン（await なし prefetch）

v5.40.0 以降、pending クエリも dehydrate 可能。await せずに prefetch を開始し、streaming SSR と併用できる。

```tsx
// await しないパターン
export default function PostsPage() {
  const queryClient = getQueryClient()
  // await なし — streaming SSR で結果が到着次第表示
  queryClient.prefetchQuery({ queryKey: ['posts'], queryFn: getPosts })
  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Posts />
    </HydrationBoundary>
  )
}
```

クライアント側は `useSuspenseQuery` を使用:

```tsx
'use client'
export default function Posts() {
  const { data } = useSuspenseQuery({ queryKey: ['posts'], queryFn: getPosts })
}
```

### 4. `@tanstack/react-query-next-experimental` について

**判断: 不要（非採用）**

- prefetch を完全にスキップし、Client Component 内で `useSuspenseQuery` を呼ぶだけで streaming SSR を実現するパッケージ
- DX は良いが、**ページナビゲーション時にリクエストウォーターフォールが発生する**
- 公式ドキュメントでも「prefetch パターンを推奨」と明記されている
- BoardFlow では Server Component での prefetch パターンが既に自然に適合するため、このパッケージは不要

### 5. `openapi-react-query` — openapi-fetch 公式の TanStack Query ラッパー

**判断: 採用推奨**

`openapi-react-query` (v0.5.4) は openapi-fetch + openapi-typescript エコシステムの公式パッケージ。1KB のラッパーで、型安全な TanStack Query フックを提供する。

#### セットアップ

```ts
import createFetchClient from "openapi-fetch"
import createClient from "openapi-react-query"
import type { paths } from "./schema"

const fetchClient = createFetchClient<paths>({
  baseUrl: "",
  credentials: "same-origin",
})
export const $api = createClient(fetchClient)
```

#### 提供される API

| メソッド | 説明 |
|---|---|
| `$api.useQuery(method, path, options)` | 型安全な useQuery |
| `$api.useSuspenseQuery(method, path, options)` | 型安全な useSuspenseQuery |
| `$api.useMutation(method, path)` | 型安全な useMutation |
| `$api.queryOptions(method, path, options)` | 型安全な queryOptions（prefetch等に使用） |
| `$api.useInfiniteQuery(method, path, options)` | 型安全な useInfiniteQuery |

#### queryKey の自動生成

`queryOptions` / `useQuery` が自動で `[method, path, params]` 形式の queryKey を生成する。手動で queryKey を設計する必要がない。

#### Server Component での prefetch との組み合わせ

```tsx
// Server Component
import { $api } from "@/lib/api/react-query-client"
import { getQueryClient } from "@/lib/query-client"

export default async function RepositoriesPage() {
  const queryClient = getQueryClient()

  await queryClient.prefetchQuery(
    $api.queryOptions("get", "/api/v1/repositories")
  )

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <RepositoriesList />
    </HydrationBoundary>
  )
}
```

```tsx
// Client Component
'use client'
import { $api } from "@/lib/api/react-query-client"

function RepositoriesList() {
  const { data } = $api.useQuery("get", "/api/v1/repositories")
  // data は prefetch 済みなので即座に利用可能
}
```

**注意**: Server Component で `$api.queryOptions()` を使う場合、サーバー側の openapi-fetch クライアントには Cookie 転送などの設定が必要。クライアント用とサーバー用で fetchClient を分けるか、prefetch の queryFn だけサーバー用クライアントを使う工夫が必要。

### 6. staleTime / gcTime のベストプラクティス

| 設定 | 推奨値 | 理由 |
|---|---|---|
| `staleTime` | `60 * 1000`（1分） | SSR では 0 だとクライアントで即座に refetch が走る。1分以上を推奨 |
| `gcTime` | `5 * 60 * 1000`（5分） | 未使用クエリのガベージコレクション。サーバー側はデフォルト Infinity |
| `refetchOnWindowFocus` | `false`（推奨） | SSR アプリでは不要なケースが多い |
| `retry` | `2` | デフォルト 3 から少し減らす |

**サーバー側の gcTime に関する注意:**
- サーバーではデフォルト `Infinity`（リクエスト完了後に自動クリア）
- 明示的に `Infinity` 以外を設定した場合は `queryClient.clear()` を呼ぶ必要がある
- `gcTime: 0` は **設定してはいけない**（hydration エラーの原因）

### 7. Data Ownership と Revalidation

公式ドキュメントが強く警告するポイント:

- Server Component で `fetchQuery()` して結果を直接レンダリングしつつ、同じデータを Client Component でも `useQuery` で使うと、**クライアント側の revalidation 時にデータが不整合になる**
- **推奨**: Server Component は prefetch の場所として扱い、データの直接レンダリングは避ける（`prefetchQuery` を使い、`fetchQuery` の結果を直接使わない）
- BoardFlow の現状は Server Component で直接 fetch → 結果を直接レンダリングしているため、TanStack Query 導入時にこのパターンを意識する必要がある

### 8. BoardFlow 固有の考慮事項

#### 現在のアーキテクチャ

| ページ | 現在のパターン | TanStack Query 移行方針 |
|---|---|---|
| リポジトリ一覧 | Server Component + createServerClient | prefetch + HydrationBoundary（必要に応じて） |
| リポジトリ詳細 | Server Component + createServerClient | 同上 |
| ボードプロジェクト詳細 | Server Component + createServerClient | 同上 |
| ラン詳細 | Server Component + createServerClient | 同上 |
| ラン diff | Server Component + createServerClient | 同上 |
| チェック詳細 | Server Component + createServerClient | 同上 |
| トークン管理 | Server Component + createServerClient (一覧) / apiClient (作成・取消) | useQuery + useMutation |
| artifact-viewer-section | useEffect + fetch (presigned URL リフレッシュ) | useQuery (refetchInterval or staleTime ベース) |

#### Server Component での prefetch 時の Cookie 転送問題

現在 `createServerClient()` は `cookies()` から `boardflow_session` を取得して API リクエストに付与している。TanStack Query の prefetch で `$api.queryOptions()` を使う場合、**サーバー側の fetchClient にも同様の Cookie 転送が必要**。

方針案:
1. **サーバー用 `$api` を別途作成**: `createServerClient()` と同様に Cookie を付与する fetchClient で `openapi-react-query` の `createClient` を呼ぶ
2. **prefetch の queryFn だけオーバーライド**: `queryOptions` の queryFn をサーバー用 fetchClient に差し替える
3. **方針1を推奨**: 一貫性が高く、queryKey が自動的に一致する

#### artifact-viewer-section.tsx のリフレッシュ

- 現在: `useEffect` + `setTimeout` で presigned URL の有効期限5分前にリフレッシュ
- TanStack Query 移行後: `useQuery` + `refetchInterval` or `staleTime` を有効期限に合わせて設定
- presigned URL の有効期限管理は TanStack Query の通常のキャッシュとは異なる特殊ケース。`staleTime` を URL の残り有効期限に合わせるか、`refetchInterval` で定期的にリフレッシュする

### 9. 必要なパッケージ

```bash
pnpm add @tanstack/react-query @tanstack/react-query-devtools openapi-react-query
```

| パッケージ | 用途 | サイズ |
|---|---|---|
| `@tanstack/react-query` | コアライブラリ | ~12KB gzipped |
| `@tanstack/react-query-devtools` | 開発用デバッグツール | devDependencies 相当（tree-shaken in prod） |
| `openapi-react-query` | openapi-fetch ↔ TanStack Query ブリッジ | ~1KB |

`@tanstack/react-query-next-experimental` は **不要**。

## BoardFlow への示唆

### 実装ステップ案

1. **基盤セットアップ**: `get-query-client.ts`, `providers.tsx` の作成、`layout.tsx` への Provider 組み込み
2. **openapi-react-query クライアント作成**: クライアント用 `$api` とサーバー用 `$apiServer` を作成
3. **段階的移行**: まず artifact-viewer-section.tsx の useEffect+fetch を useQuery に移行（最も効果が大きい）
4. **Server Component ページの移行**: 必要に応じて prefetch + HydrationBoundary パターンに移行（現在の Server Component 直接 fetch でも動作するため、優先度は低い）
5. **DevTools 組み込み**: 開発環境でのデバッグ効率向上

### 移行の優先順位

1. **高**: TanStack Query 基盤セットアップ（Provider, QueryClient）
2. **高**: `openapi-react-query` クライアント作成
3. **高**: `artifact-viewer-section.tsx` の useEffect+fetch 移行
4. **中**: トークン管理の作成・取消操作を `useMutation` に移行
5. **低**: Server Component ページの prefetch + HydrationBoundary 移行（現在動作しているため急がない）

## 採用/不採用判断

| 技術 | 判断 | 理由 |
|---|---|---|
| `@tanstack/react-query` v5 | **採用** | データフェッチング・キャッシュ・状態管理の標準ライブラリ |
| `openapi-react-query` | **採用** | openapi-fetch との公式統合。型安全性を維持したまま TanStack Query を使える |
| `@tanstack/react-query-devtools` | **採用** | 開発効率向上。本番ではバンドルに含まれない |
| `@tanstack/react-query-next-experimental` | **不採用** | prefetch パターンが推奨。ナビゲーション時のウォーターフォール問題がある |
| カスタム useGetQuery/usePostMutation フック | **不採用** | `openapi-react-query` が公式で同等以上の機能を提供 |
| `hey-api/openapi-ts` の TanStack Query プラグイン | **不採用** | 既存の openapi-typescript + openapi-fetch スタックを変更する必要がある |

## 制約と pitfall

1. **HydrationBoundary のボイラープレート**: Server Component で prefetch する場合、各ページに HydrationBoundary が必要。省略できない
2. **Server/Client の queryFn 不整合**: Server Component の prefetch で使う fetchClient と Client Component で使う fetchClient が異なる場合、Cookie 転送やベースURL の差異に注意
3. **Data Ownership**: Server Component でデータを直接レンダリングしつつ Client Component でも同じデータを useQuery すると、revalidation 時に不整合が発生する
4. **staleTime: 0 の罠**: SSR では staleTime が 0（デフォルト）だとクライアント hydration 直後に refetch が走り、二重リクエストになる
5. **gcTime: 0 の罠**: hydration エラーの原因になるため設定禁止
6. **React 19 との互換性**: TanStack Query v5 は React 19 に対応済み（v5.40.0+で streaming も対応）
7. **openapi-react-query のバージョン**: v0.5.4 は比較的新しいパッケージ。破壊的変更の可能性がある。`openapi-fetch` v0.13 との互換性は確認済み

## 未解決の疑問

1. **サーバー用 $api の Cookie 転送パターンの詳細設計**: `createServerClient()` のようにリクエストごとに Cookie を注入する場合、`openapi-react-query` の `createClient` をリクエストごとに生成するか、middleware で動的に Cookie を注入するかの判断
2. **既存 Server Component ページの移行スコープ**: 現在動作している Server Component 直接 fetch を、どこまで TanStack Query の prefetch パターンに移行するか（全部 or 必要なページのみ）
3. **ESLint プラグイン (`@tanstack/eslint-plugin-query`) の導入是非**: exhaustive-deps ルールなどが有用だが、必須ではない

## 参照URL

- [TanStack Query v5 Advanced SSR Guide](https://tanstack.com/query/v5/docs/framework/react/guides/advanced-ssr) — 公式。Server Components + Next.js App Router のセットアップガイド
- [TanStack Query v5 SSR Guide](https://tanstack.com/query/v5/docs/framework/react/guides/ssr) — 公式。基本的な SSR + hydration パターン
- [TanStack Query v5 Query Options Guide](https://tanstack.com/query/v5/docs/framework/react/guides/query-options) — 公式。queryOptions の使い方
- [TanStack Query Next.js App Prefetching Example](https://tanstack.com/query/v5/docs/framework/react/examples/nextjs-app-prefetching) — 公式サンプル
- [openapi-react-query ドキュメント](https://openapi-ts.dev/openapi-react-query/) — openapi-fetch 公式の TanStack Query ラッパー
- [openapi-react-query queryOptions API](https://openapi-ts.dev/openapi-react-query/query-options) — queryOptions の API リファレンス
- [openapi-react-query npm](https://www.npmjs.com/package/openapi-react-query) — npm パッケージ（v0.5.4）
- [Type-safe TanStack Query with OpenAPI](https://krassnig.dev/blog/type-safe-tanstack-query-with-openapi) — カスタムフックでの統合パターン解説
- [TanStack Query v5 with Next.js: Complete Guide](https://noqta.tn/en/tutorials/tanstack-query-v5-nextjs-data-fetching-guide-2026) — 包括的チュートリアル
