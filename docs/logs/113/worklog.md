# Issue #113: frontend: Server Component の React Query prefetch パターンを共通化する

## 経緯

- Next.js page.tsx 9ファイルで、React Query の queryOptions / serverClient.GET / notFound 判定 / secondary prefetch が手書きで繰り返されている
- ユーザー要望: 挙動変更なし、純粋リファクタリング

## 調査結果

### 2026-05-15: コードベース分析

**対象ファイル (8ページ、login除外):**

| ページ | パターン | primary | secondary |
|---|---|---|---|
| repositories/page.tsx | A: リスト | なし | repositories (1) |
| repositories/[repositoryId]/page.tsx | B: 詳細+セカンダリ | repo | projects (1) |
| .../boards/[boardProjectId]/page.tsx | B | project | runs (1) |
| .../boards/[boardProjectId]/runs/page.tsx | B | project | runs (1) |
| .../settings/tokens/page.tsx | B | repo | tokens (1) |
| .../runs/[boardRunId]/page.tsx | C: 詳細+複数 | run | artifacts, viewer, project (3) |
| .../runs/[boardRunId]/diff/page.tsx | D: セカンダリのみ | なし | diff, project (2) |
| .../runs/[boardRunId]/checks/[checkKind]/page.tsx | D | なし | findings, project (2) |

**共通の重複パターン:**

1. `getQueryClient()` + `createServerClient()` の初期化 (全8ファイル)
2. `$api.queryOptions()` で options 定義 → `serverClient.GET()` で queryFn を手書き (options と queryFn で同じ path/params を二重記述)
3. primary: `fetchQuery().catch(() => null)` + `if (!result) notFound()` (5ファイル)
4. secondary: `prefetchQuery()` で queryFn 手書き (全8ファイル、合計13箇所)
5. `<HydrationBoundary state={dehydrate(queryClient)}>` ラッパー (全8ファイル)

**クライアント側の消費:**

- クライアントコンポーネントは `$api.useSuspenseQuery('get', path, { params })` で消費
- `$api.queryOptions()` が生成する queryKey とクライアントの queryKey は同一であることが前提
- mutation 後の invalidation は `queryKey: ['get', '/api/v1/...']` パターン

**キー制約:**

- `openapi-react-query` の `$api.queryOptions()` は `queryKey` と型情報を生成するが、`queryFn` はブラウザ用 `apiClient` を使う
- サーバーサイドでは `createServerClient()` (Cookie付き) が必要なため、`queryFn` を上書きする必要がある
- この「queryOptions の queryKey は再利用するが queryFn はサーバー用に差し替え」が重複の根本原因

## 計画

→ 下記「実装計画」セクション参照

---

## 実装計画

### 目的

- page.tsx の prefetch 定型コードを共通ヘルパーに集約し、ページごとの重複を削減する
- 各ページは「何をフェッチするか」の宣言に集中させる

### 非目的

- 挙動変更 (Streaming SSR, hydration, notFound のタイミング)
- クライアントコンポーネント側の変更
- 新規 API エンドポイントの追加
- React Query のバージョンアップ

### 受け入れ条件

1. 主要 page.tsx の prefetch 実装重複が削減されている
2. primary resource と secondary resource の扱いが読み取りやすくなっている
3. Streaming SSR / hydration の既存挙動が維持されている
4. `pnpm typecheck` が通る
5. `pnpm lint` が通る
6. `pnpm build` が通る

---

### 詳細設計

#### Step 1: `serverQueryFn` ヘルパーの作成

**新規ファイル: `boardflow/src/lib/api/server-prefetch.ts`**

`$api.queryOptions()` の queryKey を活かしつつ、serverClient 用 queryFn を生成するヘルパーを提供する。

```typescript
// boardflow/src/lib/api/server-prefetch.ts
import type { QueryClient } from '@tanstack/react-query';
import { notFound } from 'next/navigation';
import type createClient from 'openapi-fetch';
import type { paths } from './schema';

type ServerClient = Awaited<ReturnType<typeof import('./server').createServerClient>>;

/**
 * サーバーサイド用の queryFn を生成する。
 * $api.queryOptions() が返す options の queryFn をサーバークライアント版に差し替える。
 */
export function serverQueryFn<
  Path extends keyof paths,
  Method extends keyof paths[Path] & string,
>(
  serverClient: ServerClient,
  method: Uppercase<Method>,
  path: Path,
  init?: any,  // openapi-fetch の FetchOptions 型
): () => Promise<any> {
  return async () => {
    const { data, error } = await (serverClient as any)[method](path, init);
    if (error) throw error;
    return data;
  };
}
```

ただし、openapi-fetch の型推論を壊さないために、よりシンプルなアプローチを採用する:

```typescript
// boardflow/src/lib/api/server-prefetch.ts
import type { FetchOptions } from 'openapi-fetch';
import { dehydrate } from '@tanstack/react-query';
import type { QueryClient } from '@tanstack/react-query';
import { notFound } from 'next/navigation';
import { createServerClient } from './server';
import { getQueryClient } from '../query-client';
import { $api } from './react-query';
import type { paths } from './schema';

type ServerClient = Awaited<ReturnType<typeof createServerClient>>;

/**
 * primary resource をフェッチし、結果が null なら notFound() を呼ぶ。
 * queryClient のキャッシュにも格納される。
 */
export async function fetchPrimaryResource<T>(
  queryClient: QueryClient,
  options: { queryKey: readonly unknown[] },
  fetchFn: () => Promise<T>,
): Promise<T> {
  const result = await queryClient
    .fetchQuery({
      ...options,
      queryFn: fetchFn,
    })
    .catch(() => null);

  if (!result) {
    notFound();
  }
  return result;
}

/**
 * secondary resource を prefetch する (await しない → Streaming SSR)。
 */
export function prefetchSecondaryResource<T>(
  queryClient: QueryClient,
  options: { queryKey: readonly unknown[] },
  fetchFn: () => Promise<T>,
): void {
  queryClient.prefetchQuery({
    ...options,
    queryFn: fetchFn,
  });
}

/**
 * serverClient.GET のラッパー。エラー時に throw する。
 */
export function makeServerFetcher<
  P extends keyof paths,
>(
  serverClient: ServerClient,
  path: P,
  init: paths[P] extends { get: { parameters: infer Params } } ? { params: Params } : never,
): () => Promise<any> {
  return async () => {
    const { data, error } = await serverClient.GET(path as any, init as any);
    if (error) throw error;
    return data;
  };
}
```

**問題点: openapi-fetch の型パラメータが複雑すぎて、汎用ラッパーは型安全性を損なう。**

#### 改善案: より実用的なアプローチ

openapi-fetch / openapi-react-query の型システムを壊さず、かつ重複を確実に減らすため、以下の 2 層設計を採用する:

---

**Layer 1: `fetchPrimary` / `prefetchSecondary` — 構造ヘルパー (型非依存)**

```typescript
// boardflow/src/lib/api/server-prefetch.ts

import type { QueryClient } from '@tanstack/react-query';
import { notFound } from 'next/navigation';

/**
 * Primary resource: await して fetchQuery し、取得失敗なら notFound()。
 * queryClient のキャッシュにも格納される。
 */
export async function fetchPrimary<T>(
  queryClient: QueryClient,
  options: { queryKey: readonly unknown[]; queryFn: () => Promise<T> },
): Promise<T> {
  const result = await queryClient.fetchQuery(options).catch(() => null);
  if (!result) {
    notFound();
  }
  return result;
}

/**
 * Secondary resource: prefetchQuery (await しない → Streaming SSR)。
 */
export function prefetchSecondary(
  queryClient: QueryClient,
  options: { queryKey: readonly unknown[]; queryFn: () => Promise<unknown> },
): void {
  queryClient.prefetchQuery(options);
}
```

**Layer 2: `serverFetchOptions` — queryOptions と serverClient queryFn の合成**

```typescript
/**
 * $api.queryOptions() の結果に、serverClient を使った queryFn を上書きする。
 * path と params の二重記述を解消する。
 *
 * @param clientOptions - $api.queryOptions() の返り値
 * @param serverFetcher - serverClient.GET() を呼ぶ関数
 * @returns queryKey + queryFn のオブジェクト
 */
export function withServerFetcher<T>(
  clientOptions: { queryKey: readonly unknown[] },
  serverFetcher: () => Promise<{ data: T | undefined; error: unknown }>,
): { queryKey: readonly unknown[]; queryFn: () => Promise<T> } {
  return {
    queryKey: clientOptions.queryKey,
    queryFn: async () => {
      const { data, error } = await serverFetcher();
      if (error) throw error;
      return data as T;
    },
  };
}
```

---

#### Step 2: 各 page.tsx のリファクタリング

##### Before (例: repositories/[repositoryId]/page.tsx)

```typescript
const queryClient = getQueryClient();
const serverClient = await createServerClient();

const repoOptions = $api.queryOptions('get', '/api/v1/repositories/{github_repository_id}', {
  params: { path: { github_repository_id: Number(repositoryId) } },
});

const repoResult = await queryClient
  .fetchQuery({
    ...repoOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/repositories/{github_repository_id}',
        { params: { path: { github_repository_id: Number(repositoryId) } } },
      );
      if (error) throw error;
      return data;
    },
  })
  .catch(() => null);
if (!repoResult) { notFound(); }

queryClient.prefetchQuery({
  ...projectsOptions,
  queryFn: async () => {
    const { data, error } = await serverClient.GET(
      '/api/v1/repositories/{github_repository_id}/board-projects',
      { params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } } },
    );
    if (error) throw new Error('Failed to fetch board projects');
    return data;
  },
});
```

##### After

```typescript
const queryClient = getQueryClient();
const serverClient = await createServerClient();

const repoOpts = withServerFetcher(
  $api.queryOptions('get', '/api/v1/repositories/{github_repository_id}', {
    params: { path: { github_repository_id: Number(repositoryId) } },
  }),
  () => serverClient.GET('/api/v1/repositories/{github_repository_id}', {
    params: { path: { github_repository_id: Number(repositoryId) } },
  }),
);

const projectsOpts = withServerFetcher(
  $api.queryOptions('get', '/api/v1/repositories/{github_repository_id}/board-projects', {
    params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
  }),
  () => serverClient.GET('/api/v1/repositories/{github_repository_id}/board-projects', {
    params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
  }),
);

await fetchPrimary(queryClient, repoOpts);
prefetchSecondary(queryClient, projectsOpts);
```

**削減効果:**
- `fetchQuery().catch(() => null)` + `if (!result) notFound()` のボイラープレート → `fetchPrimary()` 1行
- `prefetchQuery({ ...options, queryFn: async () => { ... } })` のボイラープレート → `prefetchSecondary()` 1行
- `serverClient.GET` と `$api.queryOptions` の path/params 二重記述は `withServerFetcher` で構造化されるが、path 文字列自体の記述は残る (openapi-fetch の型推論を維持するため)

---

#### Step 3: 変更ファイル一覧と順序

**依存関係を考慮した変更順序:**

| 順序 | ファイル | 操作 | 内容 |
|---|---|---|---|
| 1 | `src/lib/api/server-prefetch.ts` | **新規作成** | `fetchPrimary`, `prefetchSecondary`, `withServerFetcher` |
| 2 | `src/app/(authenticated)/repositories/page.tsx` | 変更 | パターンA: `withServerFetcher` + `prefetchSecondary` 適用 |
| 3 | `src/app/(authenticated)/repositories/[repositoryId]/page.tsx` | 変更 | パターンB: `fetchPrimary` + `prefetchSecondary` 適用 |
| 4 | `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx` | 変更 | パターンB |
| 5 | `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx` | 変更 | パターンB |
| 6 | `src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx` | 変更 | パターンB |
| 7 | `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx` | 変更 | パターンC: `fetchPrimary` + 3x `prefetchSecondary` |
| 8 | `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx` | 変更 | パターンD: 2x `prefetchSecondary` |
| 9 | `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx` | 変更 | パターンD: 2x `prefetchSecondary` |

**変更しないファイル:**
- `src/lib/api/react-query.ts` — 変更なし
- `src/lib/api/server.ts` — 変更なし
- `src/lib/api/client.ts` — 変更なし
- `src/lib/query-client.ts` — 変更なし
- `src/components/**` — クライアントコンポーネントは一切変更なし

---

### 影響範囲

- **Server Component (page.tsx)**: 8ファイルの prefetch ロジック
- **ランタイム挙動**: 変更なし (同じ queryKey, 同じ fetch タイミング, 同じ dehydration)
- **Client Component**: 変更なし (useSuspenseQuery の queryKey は $api.queryOptions が生成するものと同一)
- **Query Invalidation**: 変更なし (queryKey 生成ロジックに手を加えない)

---

### テスト観点

1. **typecheck**: `pnpm typecheck` — withServerFetcher の型推論が queryKey を正しく保持していること
2. **lint**: `pnpm lint` — Biome ルール準拠
3. **build**: `pnpm build` — Server Component のビルドが通ること
4. **手動確認 (Streaming SSR)**:
   - 各ページの初回読み込みで、primary resource が即座に表示されること
   - secondary resource が Suspense fallback → streaming で表示されること
   - 存在しない primary resource にアクセスすると 404 になること
5. **手動確認 (Hydration)**:
   - ページ表示後、クライアントコンポーネントが `useSuspenseQuery` で prefetch データを受け取れること
   - mutation 後の invalidation が正しく動作すること (tokens ページ)
6. **queryKey 一致確認**:
   - `withServerFetcher` が返す queryKey が `$api.queryOptions` と同一であることを、ブラウザの React Query Devtools で確認

---

### ドキュメント更新対象

- `docs/logs/113/worklog.md` — 本ファイル (実装ごとに追記)
- `docs/frontend/summary.md` — prefetch パターンのセクションを更新 (共通ヘルパーの説明追加)

---

### 設計判断の理由

**Q: なぜ `queryFn` のラッパーだけで、API query options ファイルへの集約をしないのか?**

A: 現状 `$api.queryOptions()` はクライアントコンポーネントでも `$api.useSuspenseQuery()` でインラインに使われている。queryOptions の定義を別ファイルに集約すると、クライアントコンポーネント側も変更が必要になり、リファクタリングの範囲が拡大する。Phase 1 ではサーバーサイドの重複除去に集中し、queryOptions 集約は別 Issue で検討する。

**Q: path/params の二重記述はなぜ残るのか?**

A: openapi-fetch の `serverClient.GET(path, { params })` と openapi-react-query の `$api.queryOptions(method, path, { params })` は、異なるジェネリック型システムで動作する。これらを統一する汎用ラッパーは型安全性を損なうか、実装が非常に複雑になる。`withServerFetcher` で構造を揃えることで、コピペミスのリスクは軽減しつつ、型推論は両方とも維持する。

---

### 実装要否

**`implementation_required`**

---

### 未解決の疑問

なし (純粋リファクタリングであり、仕様・要件に関する疑問は発生していない)

---

### 残リスク

1. **queryKey の不一致リスク**: `$api.queryOptions()` と `$api.useSuspenseQuery()` が内部で同じ queryKey 生成ロジックを使っている前提。openapi-react-query のバージョンアップで変更される可能性は低いが、アップデート時に確認が必要。
2. **型推論の制限**: `withServerFetcher` の戻り値型は `{ data: T | undefined; error: unknown }` に基づくため、openapi-fetch の厳密な error 型情報は失われる。サーバーサイドでは error を throw するだけなので実用上の問題はない。
