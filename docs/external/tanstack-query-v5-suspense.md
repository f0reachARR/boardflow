# TanStack Query v5 useSuspenseQuery と Suspense モード

Issue: #65

## 要約

TanStack Query v5 は `useSuspenseQuery` フックを提供し、React Suspense と統合したデータフェッチを実現する。Next.js App Router の Streaming SSR と組み合わせる場合、Server Component で `prefetchQuery`（await なし）→ Client Component で `useSuspenseQuery` のパターンが推奨される。`openapi-react-query` も `$api.useSuspenseQuery()` を提供しており、BoardFlow の既存アーキテクチャに自然に統合可能。

## 確認した情報

### 1. useSuspenseQuery の基本

```tsx
'use client'
import { useSuspenseQuery } from '@tanstack/react-query'

function Posts() {
  const { data } = useSuspenseQuery({
    queryKey: ['posts'],
    queryFn: getPosts,
  })
  // data は undefined にならない（Suspense がローディングを処理）
  return <ul>{data.map(post => <li key={post.id}>{post.title}</li>)}</ul>
}
```

**通常の `useQuery` との違い:**

| 観点 | `useQuery` | `useSuspenseQuery` |
|---|---|---|
| `data` の型 | `TData | undefined` | `TData`（undefined なし） |
| ローディング処理 | `isLoading` で手動判定 | React `<Suspense>` に委譲 |
| エラー処理 | `error` で手動判定 | `<ErrorBoundary>` に委譲 |
| `enabled` オプション | 使用可 | **使用不可** |
| `placeholderData` | 使用可 | **使用不可** |

### 2. Next.js App Router + Streaming SSR パターン

#### Server Component（await なし prefetch）

```tsx
// app/posts/page.tsx (Server Component)
import { dehydrate, HydrationBoundary } from '@tanstack/react-query'
import { getQueryClient } from '@/lib/query-client'
import { Suspense } from 'react'

export default function PostsPage() {
  const queryClient = getQueryClient()

  // await しない — streaming で結果が到着次第 Client Component に反映
  queryClient.prefetchQuery({
    queryKey: ['posts'],
    queryFn: getPosts,
  })

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<PostsSkeleton />}>
        <PostsList />
      </Suspense>
    </HydrationBoundary>
  )
}
```

#### Client Component（useSuspenseQuery）

```tsx
// app/posts/posts-list.tsx
'use client'
import { useSuspenseQuery } from '@tanstack/react-query'

export function PostsList() {
  const { data } = useSuspenseQuery({
    queryKey: ['posts'],
    queryFn: getPosts,
  })
  return <ul>{data.map(/* ... */)}</ul>
}
```

**重要: QueryClient のデフォルト設定に pending クエリの dehydrate を含める必要がある:**

```ts
import { defaultShouldDehydrateQuery } from '@tanstack/react-query'

function makeQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 60 * 1000,
      },
      dehydrate: {
        shouldDehydrateQuery: (query) =>
          defaultShouldDehydrateQuery(query) ||
          query.state.status === 'pending',
      },
    },
  })
}
```

### 3. openapi-react-query との統合

`openapi-react-query` は `$api.useSuspenseQuery()` を提供:

```tsx
// Server Component
const queryClient = getQueryClient()
queryClient.prefetchQuery(
  $api.queryOptions("get", "/api/v1/repositories")
)

// Client Component
'use client'
const { data } = $api.useSuspenseQuery("get", "/api/v1/repositories")
```

### 4. エラーハンドリング

#### throwOnError のデフォルト挙動

`useSuspenseQuery` の `throwOnError` デフォルト:

```ts
throwOnError: (error, query) => typeof query.state.data === 'undefined'
```

- **キャッシュにデータがある場合**: エラーをスローしない（stale データを表示）
- **キャッシュにデータがない場合**: Error Boundary にエラーをスロー

#### すべてのエラーを Error Boundary で処理したい場合

```tsx
const { data, error, isFetching } = useSuspenseQuery({ queryKey, queryFn })

if (error && !isFetching) {
  throw error
}
```

#### QueryErrorResetBoundary パターン（リトライ）

```tsx
import { QueryErrorResetBoundary } from '@tanstack/react-query'
import { ErrorBoundary } from 'react-error-boundary'

function App() {
  return (
    <QueryErrorResetBoundary>
      {({ reset }) => (
        <ErrorBoundary
          onReset={reset}
          fallbackRender={({ resetErrorBoundary }) => (
            <div>
              エラーが発生しました
              <button onClick={resetErrorBoundary}>再試行</button>
            </div>
          )}
        >
          <Page />
        </ErrorBoundary>
      )}
    </QueryErrorResetBoundary>
  )
}
```

### 5. prefetch 漏れ時の挙動

`useSuspenseQuery` で prefetch し忘れた場合の影響:

- **Next.js App Router**: サーバー側で Suspend → サーバーでフェッチされるが、クライアントに hydrate されず、クライアントで再フェッチが発生
- **結果**: hydration mismatch の警告が出る可能性
- **対策**: 必ず Server Component で `prefetchQuery` を呼ぶ

### 6. Fetch-on-render vs Render-as-you-fetch

| パターン | 説明 | 推奨度 |
|---|---|---|
| Fetch-on-render | コンポーネントマウント時に fetch | `useSuspenseQuery` 単体。ウォーターフォール発生のリスク |
| Render-as-you-fetch | ルーティング時に prefetch 開始 | **推奨**。Server Component で prefetch + `useSuspenseQuery` |

TanStack Query 公式ドキュメントは「prefetch on routing callbacks」を推奨しており、Next.js App Router の Server Component での prefetch がこれに相当する。

### 7. `@tanstack/react-query-next-experimental` について

`ReactQueryStreamedHydration` を使うと、prefetch なしで Client Component の `useSuspenseQuery` だけで streaming SSR が実現できるパッケージ。

**BoardFlow では不採用（既存判断を踏襲）:**
- DX は良いが、ページナビゲーション時にリクエストウォーターフォールが発生
- 公式でも prefetch パターンを推奨
- 既存の `docs/external/tanstack-query-nextjs-app-router.md` で不採用判断済み

### 8. useQuery vs useSuspenseQuery の選択基準

| シナリオ | 推奨フック |
|---|---|
| Server Component で prefetch + streaming | `useSuspenseQuery` |
| 条件付きフェッチ（`enabled` が必要） | `useQuery` |
| placeholderData が必要 | `useQuery` |
| インタラクション起因のフェッチ | `useQuery` |
| ページ初期表示データ | `useSuspenseQuery` |

## BoardFlow への示唆

### 推奨する段階的レンダリング設計

```
Server Component (page.tsx)
├── prefetchQuery (await なし) × 複数
├── HydrationBoundary
│   ├── <Suspense fallback={<HeaderSkeleton />}>
│   │   └── HeaderSection (useSuspenseQuery)
│   ├── <Suspense fallback={<TableSkeleton />}>
│   │   └── DataTable (useSuspenseQuery)
│   └── <Suspense fallback={<DetailSkeleton />}>
│       └── DetailSection (useSuspenseQuery)
```

### ページ別の推奨パターン

| ページ | パターン |
|---|---|
| リポジトリ一覧 | `loading.tsx` で十分（単一データソース） |
| リポジトリ詳細 | `loading.tsx` + 将来的に `<Suspense>` でセクション分割 |
| ボードプロジェクト詳細 | `<Suspense>` でプロジェクト情報 / ラン一覧を分割 |
| ラン詳細 | `<Suspense>` でヘッダー / artifact / diff を分割 |
| ラン一覧 | `loading.tsx` で十分（単一テーブル） |

### error.tsx の導入

`useSuspenseQuery` のエラーは Error Boundary で処理するため、各ルートセグメントに `error.tsx` を配置:

```tsx
// app/repositories/error.tsx
'use client'
export default function Error({ error, reset }) {
  return (
    <div>
      <p>データの読み込みに失敗しました</p>
      <button onClick={reset}>再試行</button>
    </div>
  )
}
```

### 必要な追加パッケージ

```bash
pnpm add react-error-boundary
```

`react-error-boundary` は `QueryErrorResetBoundary` と組み合わせてリトライUIを実現するために使用。ただし、Next.js の `error.tsx` で基本的なエラーハンドリングは可能なため、MVP では `error.tsx` のみで十分。

## 採用/不採用判断

| 技術 | 判断 | 理由 |
|---|---|---|
| `useSuspenseQuery` | **採用** | `<Suspense>` + streaming SSR に必要。`data` が undefined にならない型安全性 |
| `$api.useSuspenseQuery()` | **採用** | openapi-react-query が提供。型安全な API 呼び出しを維持 |
| `react-error-boundary` | **MVP では不要** | Next.js の `error.tsx` で基本機能は十分 |
| `@tanstack/react-query-next-experimental` | **不採用** | ウォーターフォール問題。prefetch パターンが推奨 |

## 制約と pitfall

1. **prefetch 漏れ**: `useSuspenseQuery` を使うのに Server Component で `prefetchQuery` を忘れると、hydration mismatch や二重フェッチが発生
2. **`enabled` 非対応**: `useSuspenseQuery` は `enabled` オプションを受け付けない。条件付きフェッチが必要な場合は `useQuery` を使う
3. **エラー時の stale データ**: キャッシュにデータがある場合、デフォルトでエラーをスローしない。古いデータが表示される
4. **Suspense 境界の粒度**: 粗すぎると全体がスケルトンに、細かすぎるとボイラープレートが増える
5. **dehydrate 設定**: pending クエリの dehydrate を有効にしないと streaming が機能しない（`shouldDehydrateQuery` の設定必須）
6. **Server/Client の queryFn 不整合**: Server Component の prefetch と Client Component の useSuspenseQuery で異なる fetchClient を使うと Cookie 転送の問題が発生（`docs/external/tanstack-query-nextjs-app-router.md` で既に調査済み）

## 未解決の疑問

- 複数の `useSuspenseQuery` を同一コンポーネントで呼ぶと、直列に suspend する（ウォーターフォール）。`useSuspenseQueries` で並列化するか、コンポーネントを分割して別々の `<Suspense>` に入れるか、プロジェクトの方針を決める必要がある

## 参照URL

- https://tanstack.com/query/v5/docs/framework/react/guides/suspense （公式 Suspense ガイド）
- https://tanstack.com/query/v5/docs/framework/react/guides/advanced-ssr （公式 Advanced SSR ガイド）
- https://tanstack.com/query/v5/docs/framework/react/reference/useSuspenseQuery （API リファレンス）
