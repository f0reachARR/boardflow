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

## ドキュメント確認フェーズ（2026-05-04 docs エージェント）

### 確認対象

- `docs/frontend/summary.md`
- `docs/external/tanstack-query-nextjs-app-router.md`
- `docs/technology.md`
- `README.md`
- フロントエンド実装: `src/lib/query-client.ts`, `src/lib/api/react-query.ts`, `src/components/providers.tsx`, `src/components/repositories/repositories-list.tsx`, `src/components/artifact-viewer/artifact-viewer-section.tsx`, `src/app/(authenticated)/repositories/page.tsx`, `src/app/layout.tsx`

### ドキュメント確認

- `docs/frontend/summary.md` に TanStack Query v5 + `openapi-react-query` を採用スタックとして追記
- API 連携方針に、Server Component prefetch + `HydrationBoundary`、Client Component での `$api.useQuery()`、viewer URL 更新用 `useQuery` + `refetchInterval` を追記
- `docs/technology.md` は frontend の要約ドキュメントを参照する構成のため、本 Issue では追加更新不要と判断
- `README.md` はセットアップ・主要コマンド中心であり、TanStack Query 導入に伴う利用手順変更はないため追加更新不要と判断

### レビュー結果

- 実装と `docs/frontend/summary.md` の整合性は取れた
- 外部調査メモ `docs/external/tanstack-query-nextjs-app-router.md` は research 成果物として妥当
- 外部調査メモでは server-side cookie 転送の方針案として「サーバー用 `$api` を別途作成」を推奨しているが、実装は `queryOptions` の `queryFn` オーバーライドを採用している
- 上記差分は research 時点の選択肢と実装判断の差であり、仕様ドキュメントの不整合ではない。採用判断は本 worklog に記録して補完

### 必須修正

- なし

### 任意改善

- `docs/external/tanstack-query-nextjs-app-router.md` の BoardFlow への示唆に、実装で採用した `queryFn` オーバーライド方式を追記すると後続 Issue での参照がしやすい

### PR/完了結果

- docs_ready: true
- ドキュメント観点で PR 作成可

### 残リスク

- repositories 一覧以外の Server Component ページはまだ TanStack Query prefetch パターンへ統一されていない
- React Query DevTools は開発環境限定のため、本番トラブル時の query 状態確認は別手段が必要

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

## レビューフェーズ（2026-05-04 review エージェント）

### レビュー対象

- Issue ID: #64
- 対象: TanStack Query によるデータフェッチング・キャッシュ基盤導入

### 実施内容

- 計画、research、実装概要、対象コード、関連仕様を確認
- 参照:
  - `docs/external/tanstack-query-nextjs-app-router.md`
  - `docs/spec.md`
  - `docs/frontend/summary.md`
  - `boardflow/src/lib/query-client.ts`
  - `boardflow/src/lib/api/react-query.ts`
  - `boardflow/src/components/providers.tsx`
  - `boardflow/src/app/layout.tsx`
  - `boardflow/src/components/artifact-viewer/artifact-viewer-section.tsx`
  - `boardflow/src/app/(authenticated)/repositories/page.tsx`
  - `boardflow/src/components/repositories/repositories-list.tsx`
  - `boardflow/package.json`
- 追加確認:
  - `pnpm typecheck` ✅
  - `pnpm eslint src/` ✅
  - `pnpm build` ✅
  - Web 調査で TanStack Query v5 / Next.js App Router の prefetch + HydrationBoundary / isServer 分岐パターンを再確認

### レビュー結果

- QueryClient の isServer 分岐、HydrationBoundary、`$api.queryOptions()` と `$api.useQuery()` の queryKey 整合は概ね適切
- `artifact-viewer-section.tsx` の `useQuery` 化で、リロード導線とバックグラウンド更新は維持されている
- ただし repositories prefetch のエラー伝播に欠陥があり、サーバー側取得失敗時に失敗を隠して不正な hydrated state を作る可能性があるため、このままでは PR ready にできない

### 必須修正

1. `src/app/(authenticated)/repositories/page.tsx` の prefetch `queryFn` で `serverClient.GET()` の失敗を明示的に throw すること。
  - 現状は `data!` を返しており、`error` 時に `undefined` を成功値として返しうる。
  - その場合、repositories 一覧が正しく error state にならず、空データ扱いまたは React Query の不正状態につながる。

### 任意改善

1. `src/components/repositories/repositories-list.tsx` に pending/loading 表示を追加すること。
  - 現状は `data` 未取得かつ `error` なしの間も空一覧メッセージを描画するため、prefetch ミス時や client fallback 時に誤表示になる。
2. `src/components/providers.tsx` の `ReactQueryDevtools` を計画どおり開発環境限定かつ lazy import に寄せること。
  - 計画では本番バンドル非同梱を狙っていたが、実装は常時 import / render になっている。
3. 未参照の `src/components/ui/provider.tsx` は不要なら削除、残すなら役割を明確化すること。

### テスト不足

1. repositories prefetch の異常系確認がない。
  - 認証切れ / 5xx 時に page 側で失敗が surface されるか未検証。
2. `repositories-list.tsx` の client fallback 時の loading 表示確認がない。
3. artifact viewer の URL 更新は build/lint/typecheck のみで、`expires_at` を跨ぐ動作の自動テストがない。
4. DevTools の「開発時のみ表示」「本番非同梱」は実測確認が記録されていない。

### ドキュメント確認

- `docs/logs/64/worklog.md` は更新されている
- ただし計画で更新対象としていた `docs/frontend/summary.md` に TanStack Query 導入方針の追記がない

### plan / research / docs との不整合

1. 計画上のドキュメント更新対象に `docs/frontend/summary.md` が含まれているが未更新
2. 計画では DevTools を「開発環境のみ表示」「lazy ローディングで本番バンドルに含めない」としているが、実装は常時 import
3. artifact viewer の期限前更新は research/既存設計では `expires_at` 基準の管理が中心だったが、実装は 4 分固定 interval に簡略化されている
  - backend 側 `viewer-sources` の `expires_at` が 1 時間である点から直ちに破綻はしない
  - ただし前提依存が増えたため、期限が将来短縮された際の安全性は明文化しておいた方がよい

### PR / 完了結果

- `pr_ready: false`
- 理由: repositories prefetch のエラー伝播欠落は、SSR/CSR で失敗を隠して誤表示または不正 cache state を作る可能性があり、Issue の「適切なデータフェッチ基盤導入」を満たし切れていないため

### 残リスク

1. `openapi-fetch` 0.17.0 への更新は今回の touched path では明確な破綻は見えないが、型推論や baseUrl 周辺の upstream 変更点に対する回帰確認は薄い
2. repositories 一覧以外の read 画面はまだ TanStack Query へ移行されておらず、取得方式が混在したまま
3. viewer URL 更新ロジックは backend の 1 時間 TTL に依存しており、TTL 変更時にフロントが気づきにくい

### 更新した作業ログパス

`docs/logs/64/worklog.md`

---

## レビューフェーズ（2026-05-04 review エージェント, follow-up）

### Issueまでの経緯

- 2026-05-04 初回レビューでは repositories prefetch のエラー伝播欠落を High として指摘し、`pr_ready: false` と判定した
- ユーザーから当該指摘への修正完了報告を受け、再レビューを実施した

### ユーザー要望

- 前回の必須修正が正しく解消されたか確認する
- 他に blocking な問題がなければ `pr_ready: true` を返す
- レビュー結果を `docs/logs/64/worklog.md` に追記する

### 調査結果

- `src/app/(authenticated)/repositories/page.tsx` の prefetch `queryFn` で `serverClient.GET()` の戻り値から `error` を明示判定し、失敗時に throw する実装へ修正済み
- `src/components/repositories/repositories-list.tsx` で `isPending` を見て Spinner を表示する分岐が追加済み
- `src/components/providers.tsx` で `ReactQueryDevtools` は `process.env.NODE_ENV === "development"` 条件下のみ描画される
- `get_errors` では再レビュー対象 3 ファイルにエラーなし
- 外部調査で再確認した TanStack Query の SSR/Hydration 推奨パターンとも、prefetch 時の失敗を throw して hydration させない方針は整合する

### 計画

- 前回 High 指摘の解消確認を最優先とする
- 追加で Medium/Low の修正有無、仕様・research・ドキュメントとの差分、残リスクを確認する
- blocking がなければ PR 作成可と判定する

### 実装内容

- 実装自体の追加変更はなし
- 再レビューとして、以下の修正反映を確認した
  1. repositories prefetch のエラー伝播修正
  2. repositories list の pending 表示追加
  3. React Query DevTools の development 限定表示

### テスト結果

- ユーザー報告:
  - `pnpm tsc --noEmit`: ✅
  - `pnpm eslint src/`: ✅
- 再レビュー時確認:
  - 対象ファイルの診断エラー: なし

### レビュー結果

- 前回の High 指摘は解消済み。`src/app/(authenticated)/repositories/page.tsx` で取得失敗時に throw するため、SSR prefetch 失敗を成功扱いで hydration する問題は解消された
- 前回の Medium 指摘も解消済み。prefetch 漏れや client fallback 時に空状態を誤表示せず、pending を明示できる
- DevTools の修正も受け入れ可能。lazy import ではないため計画との差分は残るが、少なくとも本番常時描画の問題は解消されており blocking ではない
- 以上より、PR を止める水準の問題は今回の確認範囲では見当たらない

### 必須修正

- なし

### 任意改善

1. `src/components/providers.tsx` の DevTools は計画どおり lazy import にすると、本番バンドル混入リスクをさらに下げられる
2. `docs/frontend/summary.md` には TanStack Query 導入方針の追記がまだなく、計画との軽微なズレが残っている

### テスト不足

1. repositories prefetch の異常系を自動テストまたは手動確認記録で残せていない
2. hydration 後に repositories 一覧で追加 fetch が発生しないことの確認記録は未記載
3. DevTools の本番非表示は条件分岐上は妥当だが、本番ビルドでの実測記録はない

### ドキュメント確認

- `docs/spec.md`: 今回修正と矛盾なし
- `docs/external/tanstack-query-nextjs-app-router.md`: 実装方針と概ね整合
- `docs/frontend/summary.md`: TanStack Query 導入の追記は未確認

### PR/完了結果

- `pr_ready: true`
- 理由: 前回の blocking 指摘が解消され、残件は任意改善または記録不足の範囲に留まるため

### 残リスク

1. DevTools は conditional render で十分実用的だが、静的 import を残しているため最適化余地はある
2. `docs/frontend/summary.md` 未更新により、採用済み基盤の認識がドキュメントで追いついていない
3. repositories 以外では取得方式が混在しており、今後の移行時にパターン統一が必要

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

---

## PR作成フェーズ（2026-05-04 pr エージェント）

### PR/完了結果

- **PR作成**: https://github.com/f0reachARR/boardflow/pull/70
- タイトル: `feat(frontend): TanStack Query v5によるデータフェッチング・キャッシュ基盤導入 (#64)`
- ベースブランチ: `main`
- ソースブランチ: `feat/64-tanstack-query`
- ラベル: `frontend`
- `Closes #64` 記載済み

### 確認事項

- ✅ `pr_ready: true`（reviewエージェント確認済み）
- ✅ `docs_ready: true`（docsエージェント確認済み）
- ✅ 未コミット変更なし（`git status` クリーン）
- ✅ `pnpm tsc --noEmit` パス
- ✅ `pnpm eslint src/` パス
- ✅ `pnpm build` 成功
- ✅ ブランチプッシュ済み

### 残リスク

- repositories 以外の Server Component ページは TanStack Query prefetch パターン未統一（後続 Issue）
- トークン管理等の mutation 系は useMutation 移行未着手（後続 Issue）
- openapi-fetch 0.13→0.17 の実行時挙動変更の可能性（統合テストで確認要）
