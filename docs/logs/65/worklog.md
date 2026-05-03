# Issue #65: Streaming SSRとローディングUI実装

## Issueまでの経緯

- 現在のServer Componentは全データ取得完了までレンダリングがブロックされる
- loading.tsxやSuspense boundaryが未導入
- スケルトンUI等のローディング表示がない
- Chakra UI v3にはSkeletonコンポーネントが存在する
- Issue #64 (TanStack Query) との連携が前提

## ユーザー要望

重いfetchについてはStreaming SSRやフロントエンドでのfetchを活用し、読み込み中のUIも適切に表示する。

## Issue作成内容

- Issue #65として新規作成
- labels: frontend
- loading.tsx/Suspense導入、スケルトンUI実装、段階的レンダリング

## 後続処理タイプの初期仮説

`implementation_required`

## 残リスク

- Issue #64 (TanStack Query) への依存関係（先にQueryインフラを整備する必要）
- Suspense boundaries の適切な粒度設計
- SSR + Streaming時のSEO考慮

---

## 実装作業ログ (2026-05-04)

### フェーズ: Research
- 開始: 2026-05-04
- 対象: Chakra UI v3 Skeleton, Next.js Streaming SSR, TanStack Query Suspense mode

#### 調査結果サマリー

**1. Chakra UI v3 Skeleton コンポーネント** (`docs/external/chakra-ui-v3-skeleton.md`)
- `Skeleton`, `SkeletonCircle`, `SkeletonText` の3コンポーネントが `@chakra-ui/react` に含まれる（追加パッケージ不要）
- `loading` prop で表示/非表示をトグル可能。`variant` は `pulse`(デフォルト) / `shine` / `none`
- テーブル用: `Table.Root` の行内に `<Skeleton height="20px" />` を配置するパターン
- カード用: `SkeletonCircle` + `SkeletonText` + `Skeleton` の組み合わせ
- 注意: Client Component 必須。CLS 対策としてスケルトンの寸法を実コンテンツに合わせる

**2. Next.js 15 Streaming SSR** (`docs/external/nextjs-streaming-ssr-loading.md`)
- `loading.tsx`: ディレクトリに配置するだけでページ全体を `<Suspense>` でラップする簡便な仕組み
- 手動 `<Suspense>`: 並列ストリーミング（sibling boundaries）、ネスト（progressive detail）が可能
- **使い分け**: `loading.tsx` はページ全体のフォールバック、`<Suspense>` は粒度の細かい制御向き
- **推奨**: 明示的 `<Suspense>` を動的アクセスの近くに配置。動的アクセスは下位コンポーネントに押し下げる
- エラーは `error.tsx` がストリーミング途中でもキャッチ。失敗セクションだけ置換、残りは影響なし
- SEO: ストリーミングはサーバーレンダリングなので影響なし。メタデータはボット向けにストリーミング前に解決
- VPS デプロイ: Node.js サーバーではネイティブサポート。Nginx リバースプロキシでは `X-Accel-Buffering: no` が必要

**3. TanStack Query v5 useSuspenseQuery** (`docs/external/tanstack-query-v5-suspense.md`)
- `useSuspenseQuery` は `data` が `TData`（undefined なし）で型安全。ローディングは `<Suspense>` に委譲
- **Streaming SSR パターン**: Server Component で `prefetchQuery`（await なし）→ Client Component で `useSuspenseQuery`
- `openapi-react-query` は `$api.useSuspenseQuery()` を提供。BoardFlow の既存アーキテクチャに統合可能
- **dehydrate 設定必須**: `shouldDehydrateQuery` で pending クエリも含めないと streaming が機能しない
- **エラー**: キャッシュにデータがあればエラーをスローしない（stale データ表示）。`QueryErrorResetBoundary` + `ErrorBoundary` でリトライ UI
- **prefetch 漏れ注意**: 忘れると hydration mismatch や二重フェッチが発生
- `@tanstack/react-query-next-experimental` は不採用（ウォーターフォール問題、既存判断踏襲）

#### 推奨する段階的レンダリング設計

| ページ | パターン |
|---|---|
| リポジトリ一覧 | `loading.tsx` + prefetch + `useSuspenseQuery` |
| リポジトリ詳細 | `loading.tsx` + 将来的に `<Suspense>` 分割 |
| ボードプロジェクト詳細 | `<Suspense>` でプロジェクト情報 / ラン一覧を分割 |
| ラン詳細 | `<Suspense>` でヘッダー / artifact / diff を分割 |
| ラン一覧 | `loading.tsx` |

#### 結論ステータス

`implementation_required` — 3トピックとも調査完了。実装に必要な情報が揃っている。

#### 残リスク
- Issue #64 (TanStack Query 基盤) が先行する必要あり（QueryClient 設定の `shouldDehydrateQuery` 等）→ **解決済み**: `query-client.ts` に設定確認済み
- 複数 `useSuspenseQuery` の直列 suspend 問題の方針決定が必要
- Nginx リバースプロキシのストリーミング設定確認
- スケルトン UI の寸法を実コンテンツに合わせる CLS 対策のデザイン検討

---

## 実装計画 (2026-05-04)

### 目的

- ページ遷移時に即座にローディングUIを表示し、UXを向上する
- 重いデータフェッチをStreaming SSRで段階的に表示する
- Chakra UI v3 Skeletonによるコンテンツのプレースホルダー表示

### 非目的

- 全ページをクライアントコンポーネントに書き換えること
- `notFound()` 判定が必要なページのサーバーフェッチを完全に排除すること
- デザインシステムの大幅な変更

### 受け入れ条件

1. 各ルートセグメントに `loading.tsx` が存在し、ナビゲーション時にスケルトンUIが即座に表示される
2. リポジトリ一覧ページが `useSuspenseQuery` + `<Suspense>` パターンで Streaming SSR に対応している
3. エラー時に `error.tsx` でユーザーフレンドリーなエラーUIが表示される
4. `pnpm typecheck`, `pnpm lint`, `pnpm build` がすべてパスする
5. CLS が最小限（スケルトンの寸法が実コンテンツに近似）

### 詳細要件

#### 現状分析

| ページ | 現在のパターン | 問題 |
|---|---|---|
| `/repositories` | Server: `await prefetchQuery` → Client: `useQuery` + Spinner | await中ページ全体ブロック。Spinner表示あるが遅い |
| `/repositories/[id]` | Server: `await Promise.all(2 fetches)` | 全データ揃うまで白画面 |
| `/repositories/[id]/boards/[id]` | Server: `await Promise.all(2 fetches)` | 同上 |
| `/repositories/[id]/boards/[id]/runs` | Server: `await Promise.all(2 fetches)` | 同上 |
| `/repositories/[id]/boards/[id]/runs/[id]` | Server: `await Promise.all(5 fetches)` | 最も重い。全データ揃うまで白画面 |

#### 変更戦略

**方針**: `loading.tsx` をすべてのルートに追加し即座にUXを改善。リポジトリ一覧を `useSuspenseQuery` に変換してStreaming SSRの先行事例とする。`notFound()` が必要なページは `loading.tsx` でのカバーに留め、page内のStreaming分割は今回スコープ外とする（将来Issue）。

**理由**: `notFound()` を使うページでStreaming SSRを実現するには、ページを「404判定用のサーバーフェッチ」と「Suspenseで囲むクライアントコンポーネント」に分離する大規模リファクタが必要。まず `loading.tsx` で全ページのナビゲーション体験を改善し、リポジトリ一覧でパターンを確立する。

### 影響範囲

- `boardflow/src/app/(authenticated)/repositories/` 以下全ルート
- `boardflow/src/components/repositories/repositories-list.tsx`
- 新規: スケルトンコンポーネント群
- 新規: `error.tsx` ファイル群

### 設計方針

1. **スケルトンコンポーネント**: `src/components/skeletons/` に再利用可能なスケルトンを配置
2. **loading.tsx**: 各ルートの `loading.tsx` からスケルトンコンポーネントを使用
3. **useSuspenseQuery 変換**: リポジトリ一覧のみ今回対象
4. **error.tsx**: 共通エラーUIコンポーネントを作成し、各ルートから使用

---

### 実装タスク一覧（順序付き）

| # | タスク | 種別 |
|---|---|---|
| 1 | スケルトンコンポーネント作成 | 新規 |
| 2 | 共通エラーUIコンポーネント作成 | 新規 |
| 3 | `/repositories` に `loading.tsx` 配置 | 新規 |
| 4 | `/repositories/[repositoryId]` に `loading.tsx` 配置 | 新規 |
| 5 | `/repositories/[repositoryId]/boards/[boardProjectId]` に `loading.tsx` 配置 | 新規 |
| 6 | `/repositories/[repositoryId]/boards/[boardProjectId]/runs` に `loading.tsx` 配置 | 新規 |
| 7 | `/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]` に `loading.tsx` 配置 | 新規 |
| 8 | 各ルートに `error.tsx` 配置 | 新規 |
| 9 | `RepositoriesList` を `useSuspenseQuery` に変換 | 修正 |
| 10 | `/repositories/page.tsx` を Streaming SSR パターンに変換 | 修正 |
| 11 | typecheck / lint / build 確認 | 検証 |

---

### 新規作成ファイル一覧

```
boardflow/src/components/skeletons/
├── repositories-table-skeleton.tsx     # リポジトリ一覧テーブルのスケルトン
├── repository-detail-skeleton.tsx      # リポジトリ詳細のスケルトン
├── board-project-detail-skeleton.tsx   # ボードプロジェクト詳細のスケルトン
├── runs-table-skeleton.tsx             # ラン一覧テーブルのスケルトン
└── run-detail-skeleton.tsx             # ラン詳細のスケルトン

boardflow/src/components/error-boundary.tsx  # 共通エラーUIコンポーネント

boardflow/src/app/(authenticated)/repositories/
├── loading.tsx
├── error.tsx
└── [repositoryId]/
    ├── loading.tsx
    ├── error.tsx
    └── boards/[boardProjectId]/
        ├── loading.tsx
        ├── error.tsx
        └── runs/
            ├── loading.tsx
            ├── error.tsx
            └── [boardRunId]/
                ├── loading.tsx
                └── error.tsx
```

### 修正ファイル一覧

```
boardflow/src/components/repositories/repositories-list.tsx
  → useQuery → useSuspenseQuery に変更、isPending/error ハンドリング削除

boardflow/src/app/(authenticated)/repositories/page.tsx
  → await prefetchQuery → prefetchQuery (await なし) + <Suspense> ラップ
```

---

### 各タスクの実装詳細

#### タスク 1: スケルトンコンポーネント作成

**`repositories-table-skeleton.tsx`**:
```tsx
import { Box, Heading, Table, Skeleton } from "@chakra-ui/react"

export function RepositoriesTableSkeleton() {
  return (
    <Box>
      <Heading size="lg" mb={6}>Repositories</Heading>
      <Table.Root size="sm" variant="outline">
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeader>Repository</Table.ColumnHeader>
            <Table.ColumnHeader>Projects</Table.ColumnHeader>
            <Table.ColumnHeader>Latest Status</Table.ColumnHeader>
            <Table.ColumnHeader>Updated</Table.ColumnHeader>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {Array.from({ length: 5 }).map((_, i) => (
            <Table.Row key={i}>
              <Table.Cell><Skeleton height="20px" width="200px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="30px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="80px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="100px" /></Table.Cell>
            </Table.Row>
          ))}
        </Table.Body>
      </Table.Root>
    </Box>
  )
}
```

他のスケルトンも同様に実コンテンツのレイアウトに近い構造で作成。

#### タスク 2: 共通エラーUIコンポーネント

```tsx
'use client'
import { Box, Heading, Text, Button, VStack } from "@chakra-ui/react"

export default function ErrorUI({ error, reset }: { error: Error; reset: () => void }) {
  return (
    <Box py={8}>
      <VStack gap={4}>
        <Heading size="md">Something went wrong</Heading>
        <Text color="gray.600">{error.message}</Text>
        <Button onClick={reset}>Try again</Button>
      </VStack>
    </Box>
  )
}
```

#### タスク 3-7: loading.tsx 配置

各 `loading.tsx` は対応するスケルトンコンポーネントをインポートして返す。例:

```tsx
// app/(authenticated)/repositories/loading.tsx
import { RepositoriesTableSkeleton } from "@/components/skeletons/repositories-table-skeleton"

export default function Loading() {
  return <RepositoriesTableSkeleton />
}
```

#### タスク 8: error.tsx 配置

各 `error.tsx` は共通の `ErrorUI` コンポーネントを使用:

```tsx
'use client'
import ErrorUI from "@/components/error-boundary"

export default function Error({ error, reset }: { error: Error; reset: () => void }) {
  return <ErrorUI error={error} reset={reset} />
}
```

#### タスク 9: RepositoriesList を useSuspenseQuery に変換

```tsx
'use client'
import { Box, Heading, Table, Text, Badge } from "@chakra-ui/react"
import Link from "next/link"
import { $api } from "@/lib/api/react-query"

export function RepositoriesList() {
  // useSuspenseQuery: data は undefined にならない。ローディングは <Suspense> に委譲
  const { data } = $api.useSuspenseQuery("get", "/api/v1/repositories", {
    params: { query: { limit: 50 } },
  })

  const repositories = data?.items ?? []
  // isPending / error ハンドリング削除（Suspense/ErrorBoundary に委譲）
  // 以下は既存のレンダリングロジックをそのまま維持
  ...
}
```

#### タスク 10: repositories/page.tsx を Streaming SSR に変換

```tsx
import { dehydrate, HydrationBoundary } from "@tanstack/react-query"
import { Suspense } from "react"
import { getQueryClient } from "@/lib/query-client"
import { createServerClient } from "@/lib/api/server"
import { $api } from "@/lib/api/react-query"
import { RepositoriesList } from "@/components/repositories/repositories-list"
import { RepositoriesTableSkeleton } from "@/components/skeletons/repositories-table-skeleton"

export default async function RepositoriesPage() {
  const queryClient = getQueryClient()
  const serverClient = await createServerClient()

  const options = $api.queryOptions("get", "/api/v1/repositories", {
    params: { query: { limit: 50 } },
  })

  // await しない → Streaming SSR で結果が到着次第クライアントに反映
  queryClient.prefetchQuery({
    ...options,
    queryFn: async () => {
      const { data, error } = await serverClient.GET("/api/v1/repositories", {
        params: { query: { limit: 50 } },
      })
      if (error) throw new Error("Failed to fetch repositories")
      return data
    },
  })

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<RepositoriesTableSkeleton />}>
        <RepositoriesList />
      </Suspense>
    </HydrationBoundary>
  )
}
```

**変更点**: `await queryClient.prefetchQuery(...)` → `queryClient.prefetchQuery(...)` (await 削除)、`<Suspense>` でラップ

---

### テスト観点

| 観点 | 方法 |
|---|---|
| 型安全性 | `pnpm typecheck` (tsc --noEmit) |
| Lint | `pnpm lint` |
| ビルド成功 | `pnpm build` |
| ナビゲーション時のローディングUI | 手動確認: 各ページ遷移時にスケルトンが表示される |
| Streaming SSR | 手動確認: リポジトリ一覧でスケルトン→データ表示の遷移 |
| エラー時のUI | 手動確認: API停止状態でエラーUIが表示される |
| CLS | 手動確認: スケルトン→実コンテンツ切替時にレイアウトシフトがない |

### ドキュメント更新対象

- `docs/frontend/summary.md` — Streaming SSR パターンの採用を追記
- `docs/logs/65/worklog.md` — 本計画と実装結果を追記

### 実装要否

`implementation_required`

### 未解決の疑問

なし — 既存の research 成果物と現在のコード構造から、実装に必要な情報はすべて揃っている。

### 更新した作業ログパス

`docs/logs/65/worklog.md`
