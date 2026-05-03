# Issue #64: TanStack Queryによるデータフェッチング・キャッシュ基盤導入

## Issueまでの経緯

- 現在のフロントエンドはServer Component + openapi-fetch（SSR）とuseEffect + fetch（Client）が混在
- artifact-viewer-section.tsxなどでuseEffect + fetchパターンが使用されている
- openapi-fetch + schema.d.tsで型安全なAPIクライアントは確立済み
- キャッシュ・再取得制御・ローディング状態管理はすべて手動
- TanStack Query未導入、関連既存Issueなし

## ユーザー要望

フロントエンドについて、useEffect + fetchではなくTanStack Queryなどを利用して、データフェッチングとキャッシュをDehydrationも活用し適切に行う。

## Issue作成内容

- Issue #64として新規作成
- labels: frontend
- TanStack Query v5導入、QueryClientProvider設定、Dehydration/Hydration、既存ページ移行

## 後続処理タイプの初期仮説

`implementation_required`

---

## 調査フェーズ（2026-05-03 research エージェント）

### 調査対象

1. TanStack Query v5 + Next.js 15 App Router 統合パターン
2. openapi-fetch との組み合わせ（openapi-react-query パッケージ）
3. QueryClient セットアップ、staleTime/gcTime ベストプラクティス
4. `@tanstack/react-query-next-experimental` の必要性

### 調査結果サマリ

#### QueryClient セットアップ
- サーバー: リクエストごとに新規 QueryClient 作成
- ブラウザ: シングルトン（isServer で分岐）
- useState での初期化は避ける（Suspense 問題）
- 推奨 staleTime: 60秒、gcTime: 5分

#### Prefetch + Dehydration パターン
- Server Component で `queryClient.prefetchQuery()` → `<HydrationBoundary state={dehydrate(queryClient)}>` でラップ
- Client Component で `useQuery` / `useSuspenseQuery` で同じ queryKey のデータを即座に使用
- HydrationBoundary は各ページに必要（省略不可）
- v5.40.0+ で pending クエリの dehydrate（streaming）にも対応

#### openapi-react-query（採用推奨）
- openapi-fetch 公式の TanStack Query ラッパー（1KB）
- `$api.useQuery("get", "/path", options)` で型安全な useQuery
- `$api.queryOptions("get", "/path", options)` で prefetch にも使える queryOptions を生成
- queryKey は `[method, path, params]` で自動生成
- カスタムフックを自作する必要なし

#### @tanstack/react-query-next-experimental（不採用）
- prefetch をスキップして useSuspenseQuery だけで SSR する実験的パッケージ
- ページナビゲーション時にリクエストウォーターフォールが発生するため非推奨
- 公式もprefetchパターンを推奨

### BoardFlow 固有の考慮事項

- 現在8ページが Server Component + `createServerClient()` で直接 fetch
- `artifact-viewer-section.tsx` のみ useEffect + fetch（presigned URL リフレッシュ）
- トークン管理（作成・取消）は `apiClient` で Client Component から直接 POST
- Server 用 `$api` の Cookie 転送問題を解決する必要あり

### 必要パッケージ

```
pnpm add @tanstack/react-query @tanstack/react-query-devtools openapi-react-query
```

### 移行優先順位

1. 高: 基盤セットアップ（Provider, QueryClient, openapi-react-query クライアント）
2. 高: artifact-viewer-section.tsx の useEffect+fetch → useQuery 移行
3. 中: トークン管理の useMutation 移行
4. 低: Server Component ページの prefetch + HydrationBoundary 移行

### 結論ステータス

`implementation_required`

### 成果物

- `docs/external/tanstack-query-nextjs-app-router.md` — 調査結果の詳細ドキュメント

### 残リスク

- サーバー用 `$api` の Cookie 転送パターンの詳細設計（`createServerClient()` 相当の機能をどう実現するか）
- 既存 Server Component ページの移行スコープの判断（全ページ vs 必要なページのみ）
- openapi-react-query v0.5.4 のバージョン安定性（比較的新しいパッケージ）
- ESLint プラグイン (`@tanstack/eslint-plugin-query`) の導入是非

---

## 計画フェーズ（2026-05-03 plan エージェント）

### 目的

- TanStack Query v5 をフロントエンドのデータフェッチング・キャッシュ基盤として導入する
- openapi-react-query を介して既存の openapi-fetch + schema.d.ts と統合し、型安全性を維持する
- Server Component での prefetch + Dehydration パターンを確立する
- `artifact-viewer-section.tsx` の手動キャッシュ管理（useEffect + setTimeout）を TanStack Query に移行する

### 非目的

- 全 Server Component ページの一括移行（本 Issue では repositories 一覧のみ移行例として実装）
- Server Action / Mutation の全面導入（トークン管理等は後続 Issue）
- `@tanstack/react-query-next-experimental` の導入
- ESLint プラグイン導入（後続検討）

### 受け入れ条件

1. `@tanstack/react-query`, `openapi-react-query`, `@tanstack/react-query-devtools` がインストール済み
2. QueryClientProvider が全ページで利用可能（ルートレイアウトに組み込み）
3. `artifact-viewer-section.tsx` が TanStack Query の `useQuery` + `refetchInterval` で presigned URL をリフレッシュ
4. `repositories/page.tsx` が prefetch + HydrationBoundary パターンで実装されている
5. React Query DevTools が開発環境で表示される
6. `pnpm tsc --noEmit` と `pnpm eslint .` がエラーなし

### 詳細要件

#### サーバー用 $api の Cookie 転送設計

`openapi-react-query` の `$api.queryOptions()` で Server Component から prefetch するには、サーバー用 fetchClient が必要。既存の `createServerClient()` は `cookies()` を使って Cookie を転送しているが、`openapi-react-query` の `createClient()` は fetchClient インスタンスを固定的に受け取るため、**prefetch 呼び出しごとに Cookie を注入する仕組み**が必要。

方針: **`queryOptions` の queryFn のみサーバー用クライアントでオーバーライドする**
- クライアント用 `$api` は既存 `apiClient` ベースで作成
- Server Component の prefetch では `$api.queryOptions()` で queryKey を取得し、`queryFn` だけ `createServerClient()` の結果を使う関数に差し替える
- これにより queryKey の一致が保証され、HydrationBoundary 経由で Client Component に正しくハイドレーションされる

#### artifact-viewer-section.tsx の移行方針

- 現在: props で初期データ受領 → useState + useEffect + setTimeout で5分前リフレッシュ
- 移行後: `useQuery` で `/api/viewer-sources/{boardRunId}` をフェッチ
  - `initialData` に props の初期値を渡す
  - `initialDataUpdatedAt` に現在時刻を渡す（staleTime 計算の起点）
  - `staleTime` を `(expiresAt - 5分 - now)` で動的計算
  - `refetchInterval` を `(expiresAt - 5分 - now)` に設定し、期限前に自動リフレッシュ
  - もしくはシンプルに `refetchInterval: 4 * 60 * 1000`（4分固定）で定期リフレッシュ
- API Route `/api/viewer-sources/[boardRunId]` はそのまま残す（Client → API Route → Backend の構造維持）

#### repositories 一覧ページの移行方針

- 現在: Server Component で `createServerClient().GET("/api/v1/repositories")` → 直接レンダリング
- 移行後:
  1. `page.tsx` を Server Component のまま、prefetch + HydrationBoundary を追加
  2. テーブル描画部分を Client Component `repositories-list.tsx` に分離
  3. Client Component 内で `$api.useQuery("get", "/api/v1/repositories")` でデータ取得
  4. prefetch 済みなのでクライアントで即座に表示（追加 fetch なし）

### 影響範囲

| ファイル | 変更種別 | 説明 |
|---|---|---|
| `boardflow/package.json` | 変更 | 3パッケージ追加 |
| `boardflow/src/lib/query-client.ts` | 新規 | getQueryClient (isServer分岐) |
| `boardflow/src/lib/api/react-query.ts` | 新規 | openapi-react-query $api クライアント |
| `boardflow/src/components/providers.tsx` | 新規 | QueryClientProvider + ChakraProvider 統合 |
| `boardflow/src/components/ui/provider.tsx` | 変更 | → providers.tsx に統合、このファイルは削除 or 薄いラッパーに変更 |
| `boardflow/src/app/layout.tsx` | 変更 | Provider → Providers に切り替え |
| `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx` | 変更 | useEffect+fetch → useQuery 移行 |
| `boardflow/src/app/(authenticated)/repositories/page.tsx` | 変更 | prefetch + HydrationBoundary 追加 |
| `boardflow/src/components/repositories/repositories-list.tsx` | 新規 | テーブル描画 Client Component |

### 設計方針

1. **段階的移行**: 各ステップで typecheck + lint がパスする状態を保つ
2. **既存機能の非破壊**: Server Component ページは動作を維持したまま段階的に移行
3. **Cookie 転送**: prefetch の queryFn オーバーライドで対応（$api のサーバー用インスタンスは作らない）
4. **DevTools**: 開発環境のみ表示（`ReactQueryDevtools` の lazy ローディングで本番バンドルに含めない）

### テスト観点

| 観点 | 確認方法 |
|---|---|
| TypeScript 型チェック | `pnpm tsc --noEmit` |
| ESLint | `pnpm eslint .` |
| ビルド成功 | `pnpm build` |
| artifact-viewer URL リフレッシュ | 手動確認 — presigned URL が有効期限前に更新されること |
| repositories 一覧の HydrationBoundary | 手動確認 — ページ初期表示時に追加 fetch が発生しないこと（DevTools で確認） |
| DevTools 表示 | 開発環境で React Query DevTools パネルが表示されること |
| SSR レンダリング | `view-source:` で prefetch データが HTML に含まれること |

### ドキュメント更新対象

- `docs/frontend/summary.md` — TanStack Query 導入の記載追加
- `docs/logs/64/worklog.md` — 実装結果を追記

---

## 実装計画

### Step 1: パッケージインストール

```bash
cd boardflow && pnpm add @tanstack/react-query @tanstack/react-query-devtools openapi-react-query
```

**作成/変更ファイル:**
- `boardflow/package.json` — 依存追加
- `boardflow/pnpm-lock.yaml` — 自動更新

**検証:** `pnpm tsc --noEmit` パス（パッケージ追加のみなので影響なし）

---

### Step 2: QueryClient セットアップ

**新規作成: `boardflow/src/lib/query-client.ts`**

```ts
import { isServer, QueryClient, defaultShouldDehydrateQuery } from "@tanstack/react-query"

function makeQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 60 * 1000, // 1分
        refetchOnWindowFocus: false,
      },
      dehydrate: {
        shouldDehydrateQuery: (query) =>
          defaultShouldDehydrateQuery(query) || query.state.status === "pending",
      },
    },
  })
}

let browserQueryClient: QueryClient | undefined = undefined

export function getQueryClient() {
  if (isServer) {
    return makeQueryClient()
  } else {
    if (!browserQueryClient) browserQueryClient = makeQueryClient()
    return browserQueryClient
  }
}
```

**検証:** `pnpm tsc --noEmit` パス

---

### Step 3: Providers 統合 + openapi-react-query クライアント作成

**新規作成: `boardflow/src/lib/api/react-query.ts`**

```ts
import createClient from "openapi-react-query"
import { apiClient } from "./client"

export const $api = createClient(apiClient)
```

**新規作成: `boardflow/src/components/providers.tsx`**

```tsx
"use client"

import { QueryClientProvider } from "@tanstack/react-query"
import { ReactQueryDevtools } from "@tanstack/react-query-devtools"
import { ChakraProvider, defaultSystem } from "@chakra-ui/react"
import { getQueryClient } from "@/lib/query-client"

export function Providers({ children }: { children: React.ReactNode }) {
  const queryClient = getQueryClient()
  return (
    <QueryClientProvider client={queryClient}>
      <ChakraProvider value={defaultSystem}>
        {children}
      </ChakraProvider>
      <ReactQueryDevtools />
    </QueryClientProvider>
  )
}
```

**変更: `boardflow/src/app/layout.tsx`**
- `Provider` → `Providers` に差し替え
- import 元を `@/components/providers` に変更

**変更: `boardflow/src/components/ui/provider.tsx`**
- 既存コードは残す（他で参照がなければ後日削除）
- 他コンポーネントが `Provider` を import していないことを確認

**検証:** `pnpm tsc --noEmit` + `pnpm eslint .` パス

---

### Step 4: artifact-viewer-section.tsx の TanStack Query 移行

**変更: `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx`**

変更内容:
1. `useState(initialViewers)`, `useState(initialExpiresAt)`, `useState(refreshError)` を削除
2. `useEffect` + `setTimeout` のリフレッシュロジックを削除
3. `$api.useQuery` または素の `useQuery` で `/api/viewer-sources/{boardRunId}` をフェッチ:
   - viewer-sources API Route は Next.js 内部なので `openapi-react-query` ではなく素の `useQuery` を使用
   - queryKey: `["viewer-sources", boardRunId]`
   - queryFn: `fetch(/api/viewer-sources/${boardRunId})` → JSON パース
   - initialData: props の `viewers` + `expiresAt` を構造化
   - refetchInterval: 4分（240_000ms）固定 — presigned URL の5分前リフレッシュを確実に行う
   - refetchIntervalInBackground: true（タブ非アクティブでもリフレッシュ）
4. `data` から `viewers` と `expiresAt` を取得してレンダリング
5. `isError` で refreshError 相当を判定

**注意点:**
- `/api/viewer-sources/[boardRunId]` は Next.js API Route であり OpenAPI スキーマに含まれないため、素の `useQuery` を使う
- props インターフェースは変更しない（Server Component から initialData を渡す構造は維持）

**検証:** `pnpm tsc --noEmit` + `pnpm eslint .` パス + 手動動作確認

---

### Step 5: repositories 一覧ページの prefetch + HydrationBoundary 移行

**新規作成: `boardflow/src/components/repositories/repositories-list.tsx`**

```tsx
"use client"

import { $api } from "@/lib/api/react-query"
// ... Chakra UI imports, Link etc.

export function RepositoriesList() {
  const { data, error } = $api.useQuery("get", "/api/v1/repositories", {
    params: { query: { limit: 50 } },
  })

  // 既存の repositories/page.tsx のテーブル描画ロジックを移動
}
```

**変更: `boardflow/src/app/(authenticated)/repositories/page.tsx`**

```tsx
import { dehydrate, HydrationBoundary } from "@tanstack/react-query"
import { getQueryClient } from "@/lib/query-client"
import { createServerClient } from "@/lib/api/server"
import { $api } from "@/lib/api/react-query"
import { RepositoriesList } from "@/components/repositories/repositories-list"

export default async function RepositoriesPage() {
  const queryClient = getQueryClient()
  const serverClient = await createServerClient()

  // queryKey を $api.queryOptions から取得し、queryFn はサーバー用クライアントで実行
  const options = $api.queryOptions("get", "/api/v1/repositories", {
    params: { query: { limit: 50 } },
  })

  await queryClient.prefetchQuery({
    ...options,
    queryFn: async () => {
      const { data } = await serverClient.GET("/api/v1/repositories", {
        params: { query: { limit: 50 } },
      })
      return data
    },
  })

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <RepositoriesList />
    </HydrationBoundary>
  )
}
```

**検証:** `pnpm tsc --noEmit` + `pnpm eslint .` + `pnpm build` パス

---

### Step 6: DevTools 組み込み確認 + 最終検証

- Step 3 で既に `<ReactQueryDevtools />` を Providers に組み込み済み
- 開発環境 (`pnpm dev`) で画面右下に DevTools トグルが表示されることを確認
- 本番ビルド (`pnpm build`) で DevTools が tree-shaken されることを確認（バンドルサイズ）

**最終検証:**
```bash
pnpm tsc --noEmit
pnpm eslint .
pnpm build
```

---

### 実装順序とロールバック戦略

| Step | 依存 | ロールバック容易性 |
|---|---|---|
| 1 (パッケージ追加) | なし | pnpm remove で即戻し |
| 2 (QueryClient) | Step 1 | ファイル削除のみ |
| 3 (Providers + $api) | Step 1, 2 | layout.tsx を戻せば元通り |
| 4 (artifact-viewer) | Step 1, 3 | git checkout で元ファイルに戻す |
| 5 (repositories) | Step 1, 2, 3 | git checkout + ファイル削除 |
| 6 (最終確認) | 全Step | — |

### 実装要否

`implementation_required`

### 未解決の疑問

1. **refetchInterval の値**: artifact-viewer の presigned URL リフレッシュは4分固定で十分か、それとも `expiresAt` からの動的計算が必要か → 4分固定で開始し、問題があれば調整（presigned URL は通常1時間有効なので4分間隔で十分）
2. **`openapi-react-query` の queryOptions の型**: Server Component で `$api.queryOptions()` を呼べるか（react import が不要な関数のみなら問題なし） → 公式ドキュメントで Server Component 利用例あり、問題なし

### 更新した作業ログパス

`docs/logs/64/worklog.md`

---

## 実装フェーズ（2026-05-03 impl エージェント）

### 実装内容

#### パッケージ追加
- `@tanstack/react-query` v5.100.9
- `@tanstack/react-query-devtools` v5.100.9
- `openapi-react-query` v0.5.4
- `openapi-fetch` を 0.13.8 → 0.17.0 にアップグレード（peer dep 要件）

#### 新規ファイル
| ファイル | 役割 |
|---|---|
| `src/lib/query-client.ts` | isServer 分岐の QueryClient ファクトリ（staleTime: 60s, pending dehydrate 対応） |
| `src/lib/api/react-query.ts` | `$api = createClient(apiClient)` — openapi-react-query クライアント |
| `src/components/providers.tsx` | QueryClientProvider + ChakraProvider + ReactQueryDevtools の統合 Provider |
| `src/components/repositories/repositories-list.tsx` | repositories テーブルの Client Component（$api.useQuery 使用） |

#### 変更ファイル
| ファイル | 変更内容 |
|---|---|
| `src/app/layout.tsx` | `Provider` → `Providers` に切り替え |
| `src/components/artifact-viewer/artifact-viewer-section.tsx` | useEffect+fetch+useState → useQuery（refetchInterval: 4分） |
| `src/app/(authenticated)/repositories/page.tsx` | prefetch + HydrationBoundary パターンに移行 |
| `package.json` | 依存関係追加・更新 |

#### アーキテクチャ決定
- artifact-viewer: `useQuery` で 4分間隔の定期リフレッシュ。initialData に SSR の初期値を渡す
- repositories: Server Component で prefetchQuery → HydrationBoundary → Client Component で useQuery（即座にキャッシュから表示）
- Server 用 prefetch は `queryFn` のみオーバーライドし、queryKey は $api.queryOptions() から取得して一致を保証

### テスト結果

- `pnpm tsc --noEmit`: ✅ エラーなし
- `pnpm eslint src/`: ✅ エラーなし
- `pnpm build`: ✅ 成功（全ページ正常コンパイル）

### 更新ドキュメント

- `docs/external/tanstack-query-nextjs-app-router.md` — TanStack Query + Next.js App Router の調査結果

### 残リスク

1. **openapi-fetch 0.13 → 0.17 の破壊的変更**: 型チェック・ビルドは通過したが、実行時の挙動変更がある可能性（特にエラーレスポンスのハンドリング）。統合テスト時に確認が必要。
2. **他ページの移行スコープ**: 本 Issue では repositories のみ移行。他の Server Component ページ（board-run 詳細、tokens など）は後続 Issue で対応。
3. **useMutation 未導入**: トークン作成・削除などの mutation 系は現状手動 fetch のまま。後続 Issue で対応。
4. **ESLint プラグイン未導入**: `@tanstack/eslint-plugin-query` は後続検討。
5. **`ui/provider.tsx` の残存**: 他のファイルから参照されている可能性があるため削除せず残した。layout.tsx からの参照のみ Providers に切り替え。

### 更新した作業ログパス

`docs/logs/64/worklog.md`
