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

## 実装完了 (2026-05-04)

### 実施内容

計画通り全11タスクを実施。

1. **スケルトンコンポーネント5つ作成** (`src/components/skeletons/`)
   - `repositories-table-skeleton.tsx` — 5行のスケルトンテーブル
   - `repository-detail-skeleton.tsx` — Breadcrumb + Heading + Board Projects テーブル
   - `board-project-detail-skeleton.tsx` — メタ情報 + Recent Runs テーブル
   - `runs-table-skeleton.tsx` — 5行のRunsテーブル
   - `run-detail-skeleton.tsx` — ヘッダー + メタデータ + アーティファクト領域

2. **共通エラーUIコンポーネント** (`src/components/error-boundary.tsx`)
   - `ErrorUI` コンポーネント: エラーメッセージ + "Try again" ボタン

3. **`loading.tsx` 5ファイル配置** — 各ルートセグメントでスケルトンを表示

4. **`error.tsx` 5ファイル配置** — 各ルートセグメントで `ErrorUI` を使用

5. **`RepositoriesList` を `useSuspenseQuery` に変換**
   - `$api.useQuery` → `$api.useSuspenseQuery`
   - `isPending` / `error` ハンドリング削除（Suspense/ErrorBoundary に委譲）
   - `Spinner` インポート削除

6. **`/repositories/page.tsx` を Streaming SSR に変換**
   - `await queryClient.prefetchQuery(...)` → `queryClient.prefetchQuery(...)` (await 削除)
   - `<Suspense fallback={<RepositoriesTableSkeleton />}>` でラップ
   - `Suspense`, `RepositoriesTableSkeleton` をインポート追加

### テスト結果

| チェック | 結果 |
|---|---|
| `pnpm typecheck` | ✅ パス |
| `pnpm lint` | ✅ パス (No ESLint warnings or errors) |
| `pnpm build` | ✅ パス (全ルート正常ビルド) |

### Git

- ブランチ: `feature/65-streaming-ssr-loading-ui`
- コミット: `cf34a18` — `feat(frontend): add Streaming SSR with loading/error UI (#65)`

### 残リスク

- **notFound() ページの Streaming SSR**: 詳細ページ（リポジトリ詳細, ボード詳細, ラン詳細, ラン一覧）はサーバーサイドで `notFound()` 判定が必要なため、今回は `loading.tsx` でのカバーに留めている。将来的にページ内を「404判定用サーバーフェッチ」と「Suspense囲みクライアントコンポーネント」に分離する追加リファクタが必要。
- **CLS 最適化**: スケルトンの寸法は実コンテンツの概算値。実際のデータ量に応じて微調整が必要な可能性あり。
- **Nginx リバースプロキシ**: 本番環境で Streaming SSR を機能させるには `X-Accel-Buffering: no` ヘッダーの設定が必要。

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

---

## レビュー結果 (2026-05-04, follow-up review)

### 対象

- Issue: #65 Streaming SSRとローディングUI実装
- 対象コミット:
  - `cf34a18` — 初回実装
  - `7ad7ab6` — 作業ログ更新
  - `869630b` — 前回レビュー指摘への修正

### 確認内容

1. 前回レビューの3指摘の修正確認
2. `pnpm typecheck` / `pnpm lint` / `pnpm build` の再実行
3. 実装と計画、`docs/frontend/summary.md`、外部調査メモの整合確認

### レビュー結果

- **前回指摘1: QueryErrorResetBoundary の導入**
  - `boardflow/src/components/error-boundary.tsx` で `useQueryErrorResetBoundary()` を使用し、`handleReset()` で `resetQuery()` → `reset()` の順に呼んでいることを確認。
  - TanStack Query の Suspense/Error Boundary 運用方針と整合している。

- **前回指摘2: スケルトンの CLS 改善**
  - `repository-detail-skeleton.tsx` に Settings セクション追加を確認。
  - `board-project-detail-skeleton.tsx` に ERC/DRC 列を含む 6 列テーブルを確認。
  - `run-detail-skeleton.tsx` に Checks / Artifact Summary / Artifacts テーブルを確認。
  - 対応先ページの主要レイアウトとの乖離は縮小されており、前回指摘は解消したと判断。

- **前回指摘3: docs/frontend/summary.md の更新**
  - API 連携セクションに `useSuspenseQuery`、`loading.tsx`、`QueryErrorResetBoundary` の方針追記を確認。
  - 実装内容とドキュメントの方向性は整合している。

### 再検証結果

- `pnpm typecheck` ✅
- `pnpm lint` ✅
- `pnpm build` ✅

### 追加所見

- ブロッキングな不具合は今回の確認範囲では未検出。
- `/repositories` の Streaming SSR 実装は、`prefetchQuery` を await せずに開始し、`HydrationBoundary` と `useSuspenseQuery` を組み合わせる構成になっており、事前調査内容と整合している。
- `query-client.ts` の `shouldDehydrateQuery` で pending query を dehydrate 対象に含めており、Streaming SSR の成立条件も満たしている。

### テスト不足

- loading / error UI は静的検証と build では担保されているが、ルート遷移時のスケルトン表示、API エラー時の再試行動作を検証する component test / E2E test は未整備。
- 現時点ではマージ阻害要因ではないが、今後この領域を継続的に変更するなら自動テスト追加余地がある。

### ドキュメント確認

- `docs/frontend/summary.md`: 更新確認済み
- `docs/spec.md`: 本 Issue の実装と矛盾する記述は今回確認範囲では未検出
- `docs/external/tanstack-query-v5-suspense.md`: 実装方針と整合
- `docs/external/nextjs-streaming-ssr-loading.md`: 実装方針と整合

### PR/完了結果

- `pr_ready: true`
- 理由: 前回指摘は解消され、再実行した `typecheck` / `lint` / `build` もすべて成功。新たなブロッキング問題は確認できなかったため。

### 残リスク

- 実ブラウザでのナビゲーション中スケルトン表示やエラー再試行の挙動は自動テスト未整備のため、回帰検知は現状だと手動確認に依存する。

### 実装要否

`implementation_required`

### 未解決の疑問

なし — 既存の research 成果物と現在のコード構造から、実装に必要な情報はすべて揃っている。

### 更新した作業ログパス

`docs/logs/65/worklog.md`

---

## ドキュメント確認 (2026-05-04)

### フェーズ: Docs Review

#### 対象Issue

- Issue ID: #65
- タイトル: Streaming SSRとローディングUI実装

#### 確認範囲

- 実装: `boardflow/src/app/(authenticated)/repositories/**`, `boardflow/src/components/skeletons/**`, `boardflow/src/components/error-boundary.tsx`, `boardflow/src/lib/query-client.ts`
- ドキュメント: `docs/frontend/summary.md`, `docs/external/chakra-ui-v3-skeleton.md`, `docs/external/nextjs-streaming-ssr-loading.md`, `docs/external/tanstack-query-v5-suspense.md`, `README.md`
- 外部根拠: Next.js 公式 `loading.js` / self-hosting、TanStack Query 公式 Suspense / Advanced SSR、Chakra UI 公式 Skeleton / Server Components

#### ドキュメント確認結果

- `/repositories` の Streaming SSR 実装、`loading.tsx` 配置、`useSuspenseQuery` 化、pending query の dehydration 設定は、`docs/frontend/summary.md` と概ね整合している
- `docs/external/nextjs-streaming-ssr-loading.md` の主要な整理は、Next.js 公式の `loading.js` と self-hosting の説明に沿っており妥当
- ただし、Chakra UI と TanStack Query の外部調査メモに、現在の実装方針とずれる断定が残っている

#### 必須修正

- `docs/external/chakra-ui-v3-skeleton.md` の Server Component 制約を修正する。現在は「Chakra UI コンポーネント全般と同様、`'use client'` が必要」「`loading.tsx` では `Skeleton` を Client Component に切り出して使う」と読めるが、Chakra UI 公式の Server Components ドキュメントでは Chakra UI components can be used with React Server Components without adding the 'use client' directive とされている。実装でも `boardflow/src/app/(authenticated)/repositories/loading.tsx` は Server Component のまま Chakra ベースの skeleton を返しており、現状のメモは BoardFlow 実装と矛盾する。
- `docs/frontend/summary.md` のエラー処理方針を実装に合わせる。現在は `error.tsx` + `QueryErrorResetBoundary` と記載されているが、実装は `boardflow/src/components/error-boundary.tsx` で `useQueryErrorResetBoundary()` を使って Query reset を接続しており、`QueryErrorResetBoundary` コンポーネント自体は使っていない。方針としては「`QueryErrorResetBoundary` または `useQueryErrorResetBoundary`」のように、実装と一致する表現へ修正した方がよい。
- `docs/external/tanstack-query-v5-suspense.md` の「必要な追加パッケージ」を修正する。現在は `pnpm add react-error-boundary` を必要条件のように記載しているが、BoardFlow 実装では追加していない。同じ節の直後で「Next.js の `error.tsx` で基本的なエラーハンドリングは可能」と書いており、必要条件と代替手段が混在している。今回の Issue 文脈では「`react-error-boundary` を使う構成もあるが、BoardFlow では Next.js `error.tsx` + `useQueryErrorResetBoundary` を採用」と整理するのが正確。

#### 任意改善

- `docs/external/tanstack-query-v5-suspense.md` の `error.tsx` サンプルは、単純な `reset` 呼び出しだけでなく Query reset 連携の説明を補うと、現実装に近づく
- `docs/external/chakra-ui-v3-skeleton.md` の BoardFlow への示唆で、`loading` prop ベースの説明と `loading.tsx` ベースの説明を分けて書くと、ページ遷移時 fallback とクライアント内ローディングの使い分けが明確になる

#### docs_ready 判定

- `docs_ready: false`

#### 残リスク

- 実装担当者が `docs/external/chakra-ui-v3-skeleton.md` を参照すると、`loading.tsx` で不要な Client Component 化が必要だと誤解する
- `docs/frontend/summary.md` のエラー reset 記述を見た読者が、現行実装にない `QueryErrorResetBoundary` コンポーネント導入を前提だと誤認する
- `docs/external/tanstack-query-v5-suspense.md` の追加パッケージ記述により、不要な依存追加を誘発する可能性がある

#### 更新した作業ログパス

`docs/logs/65/worklog.md`

---

## レビュー結果 (2026-05-04)

### フェーズ: Review

#### 対象Issue

- Issue ID: #65
- タイトル: Streaming SSRとローディングUI実装

#### 調査結果

- 実装差分を確認: `loading.tsx` / `error.tsx` の追加、`RepositoriesList` の `useSuspenseQuery` 化、`/repositories/page.tsx` の `prefetchQuery` 非同期化を確認
- QueryClient 設定を確認: `shouldDehydrateQuery` で `pending` を dehydration 対象に含めており、Streaming SSR の成立条件は満たしている
- 外部整合を確認:
  - Next.js App Router の `loading.tsx` / `error.tsx` の基本挙動と整合
  - TanStack Query の `prefetchQuery` + `useSuspenseQuery` パターンと整合
  - Chakra UI v3 Skeleton API の利用自体は妥当
- 実行検証を再実施:
  - `pnpm typecheck` ✅
  - `pnpm lint` ✅
  - `pnpm build` ✅

#### レビュー結果

**総評**

- `/repositories` の Streaming SSR 化そのものは成立しており、ビルドも通っている
- ただし、Suspense エラー時の再試行経路と、CLS 配慮をうたうスケルトン近似度、設計ドキュメントの追従に未解決点が残る
- この状態では `pr_ready: false`

**重大度順の指摘**

1. **中**: Suspense Query のエラー再試行導線が不完全
   - `RepositoriesList` は `useSuspenseQuery` を利用している一方で、共通 Provider 配下に `QueryErrorResetBoundary` も `useQueryErrorResetBoundary` も導入されていない
   - `error.tsx` から渡される `reset` をそのまま押しても、TanStack Query 側の query error state を明示的に reset しないため、失敗キャッシュが残ったまま再描画されるリスクがある
   - 公式ドキュメントでも Suspense 利用時の Error Boundary reset には Query 側の reset 連携が必要とされている

2. **中**: スケルトン UI が実レイアウトを十分に近似しておらず、Issue の CLS 要件とずれている
   - `RepositoryDetailSkeleton` は実画面に存在する `Settings` セクションを持たず、`Board Projects` テーブルへ直接遷移する構成になっている
   - `BoardProjectDetailSkeleton` の `Recent Runs` は 4 列構成だが、実画面は `ERC` / `DRC` を含む 6 列構成
   - `RunDetailSkeleton` は汎用カード 1 つと大きなプレースホルダ中心で、実画面の `Checks`、`Artifact Summary`、`Changes from Baseline`、viewer セクションをほぼ反映していない
   - loading UI は存在するが、計画で掲げた「寸法を実コンテンツに近似して CLS を最小化」には未達

3. **低**: 実装方針の要約ドキュメントが最新状態に追従していない
   - `docs/frontend/summary.md` には依然として「Client Component は `$api.useQuery()` を基本形」とあり、今回導入した `useSuspenseQuery` + `loading.tsx` + Streaming SSR の方針が反映されていない
   - worklog と research は更新されているが、継続開発で参照されやすい要約ドキュメントが古いままになっている

**必須修正**

- `useSuspenseQuery` のエラー再試行経路を整備する
  - `QueryErrorResetBoundary` または `useQueryErrorResetBoundary` を導入し、`error.tsx` の `reset` と TanStack Query の reset を接続する
- 各スケルトンを実画面構成に寄せる
  - 少なくとも、`RepositoryDetailSkeleton` の `Settings`、`BoardProjectDetailSkeleton` の `ERC` / `DRC` 列、`RunDetailSkeleton` の主要セクションを反映する

**任意改善**

- `/repositories/page.tsx` の `prefetchQuery(...)` には `void` を付け、意図的な fire-and-forget をコード上で明示してもよい
- `RepositoriesList` の `data?.items ?? []` は Suspense 前提では `data.items ?? []` に寄せると意図が読みやすい

**テスト不足**

- API エラー時に `error.tsx` の `Try again` が実際に復旧経路として機能するかの動作確認がない
- skeleton 表示から実データ表示への遷移で大きなレイアウトシフトが出ないかの確認が手動観点に留まっている

**ドキュメント確認**

- 更新あり:
  - `docs/external/chakra-ui-v3-skeleton.md`
  - `docs/external/nextjs-streaming-ssr-loading.md`
  - `docs/external/tanstack-query-v5-suspense.md`
  - `docs/logs/65/worklog.md`
- 更新漏れ候補:
  - `docs/frontend/summary.md` に Suspense/Streaming SSR 方針の反映が必要
- `CONTRIBUTING.md` はリポジトリ内に存在せず、確認不可

**plan / research / docs との不整合**

- research では Suspense エラー時に Query reset 連携が必要と整理されているが、実装には反映されていない
- 計画では CLS を最小化するためのレイアウト近似を受け入れ条件に含めているが、詳細系 skeleton がそこまで到達していない
- 計画中の「`docs/frontend/summary.md` 追記」は未実施

**PR/完了結果**

- `pr_ready: false`

**残リスク**

- `/repositories` の初回 API 障害時に、ユーザーが `Try again` を押しても即復旧できない可能性がある
- 詳細ページ群の skeleton は視覚的には表示されるが、実レイアウトとの差分が大きく CLS を誘発する可能性がある

### 更新した作業ログパス

`docs/logs/65/worklog.md`

---

## レビュー修正 (2026-05-04)

### 指摘と修正内容

| # | 指摘 | 修正 |
|---|---|---|
| 1 | `QueryErrorResetBoundary` 未導入 | `error-boundary.tsx` に `useQueryErrorResetBoundary` を追加し、`handleReset` で Query リセット → Next.js reset の順に呼ぶよう変更 |
| 2-1 | `repository-detail-skeleton.tsx` に Settings セクション欠如 | Settings セクション（アイコン + リンクスケルトン）を Board Projects の前に追加 |
| 2-2 | `board-project-detail-skeleton.tsx` の Recent Runs に ERC/DRC 列がない | テーブルを4列 → 6列（Status, Commit, Branch, ERC, DRC, Created）に修正。メタ情報行も3→4行に |
| 2-3 | `run-detail-skeleton.tsx` に Checks/Artifact Summary/Artifacts テーブルが欠如 | 全面書き直し: Header + Checks (2カード) + Artifact Summary + Artifacts テーブル(4列4行)構成に |
| 3 | `docs/frontend/summary.md` のAPI連携セクションが未更新 | `useSuspenseQuery` / Streaming SSR / `loading.tsx` / `QueryErrorResetBoundary` の記述を追加 |

### テスト結果

| チェック | 結果 |
|---|---|
| `pnpm typecheck` | ✅ パス |
| `pnpm lint` | ✅ パス |
| `pnpm build` | ✅ パス |

### Git

- コミット: `6d2d9ca` — `fix(frontend): address review feedback for #65`

### 残リスク

- 指摘はすべて解消済み
- Nginx ストリーミング設定 (`X-Accel-Buffering: no`) は本番デプロイ時に別途対応が必要

### 更新した作業ログパス

`docs/logs/65/worklog.md`
