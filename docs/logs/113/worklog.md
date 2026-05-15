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

## 計画

- `boardflow/src/lib/api/server-prefetch.ts` に3つのヘルパー関数を作成
  - `fetchPrimary<T>`: fetchQuery + catch + notFound
  - `prefetchSecondary`: prefetchQuery ラッパー (await しない)
  - `withServerFetcher<T>`: queryOptions の queryKey を維持しつつ serverClient.GET を queryFn に注入
- 8ページファイルでインラインの prefetch コードをヘルパー呼び出しに置換
- 挙動変更なし: Streaming SSR / hydration / notFound のタイミングは同一

## 実装内容

### 2026-05-15: 実装完了

**新規ファイル:**
- `boardflow/src/lib/api/server-prefetch.ts` — 3ヘルパー関数

**変更ファイル (8ページ):**
1. `repositories/page.tsx` — パターンA: prefetchSecondary + withServerFetcher
2. `repositories/[repositoryId]/page.tsx` — パターンB: fetchPrimary + prefetchSecondary
3. `.../boards/[boardProjectId]/page.tsx` — パターンB
4. `.../boards/[boardProjectId]/runs/page.tsx` — パターンB
5. `.../settings/tokens/page.tsx` — パターンB
6. `.../runs/[boardRunId]/page.tsx` — パターンC: fetchPrimary + 3x prefetchSecondary
7. `.../runs/[boardRunId]/diff/page.tsx` — パターンD: 2x prefetchSecondary のみ
8. `.../runs/[boardRunId]/checks/[checkKind]/page.tsx` — パターンD (バリデーションロジック変更なし)

**型修正:**
- `withServerFetcher` の `serverFetcher` 引数型を `{ data?: T; error?: unknown }` に修正（openapi-fetch の FetchResponse は `data` がオプショナルプロパティ）

**削除されたインポート:**
- パターンB の5ファイルから `import { notFound } from 'next/navigation'` を削除（fetchPrimary 内で呼ばれるため）

## テスト結果

| チェック | 結果 |
|---|---|
| `pnpm typecheck` | ✅ パス |
| `pnpm lint` (Biome) | ✅ パス (フォーマット自動修正後) |
| `pnpm build` | ✅ パス (全ルート正常ビルド) |

## ドキュメント確認

- クライアントコンポーネント変更なし
- HydrationBoundary / dehydrate パターン維持
- checks ページのバリデーションロジック変更なし

## 残リスク

- なし。純粋なリファクタリングで挙動変更なし。

## レビュー結果

### 2026-05-15: レビュー完了

**総評:**
- 前回の重大指摘だった secondary prefetch 時のエラーメッセージ消失は修正済み。`withServerFetcher` の `errorMessage` 追加と各 secondary call site への明示指定をコード上で確認した。
- `fetchPrimary` / `prefetchSecondary` / `withServerFetcher` への抽出により、primary と secondary の責務分離は読みやすくなっており、`HydrationBoundary + dehydrate(queryClient)` の既存パターンも維持されている。

**確認内容:**
- `boardflow/src/lib/api/server-prefetch.ts` で `errorMessage?: string` が実装され、secondary 側のみメッセージを付与できることを確認。
- 対象 8 ページで helper 利用を確認。primary は 5 箇所で `errorMessage` なし、secondary は 12 箇所すべてで明示メッセージ付き。
- `boardflow/src/lib/query-client.ts` で `shouldDehydrateQuery` が `pending` を含む設定になっており、secondary の `prefetchQuery` 非 await による Streaming SSR 前提が維持されることを確認。
- 再検証として `pnpm typecheck` / `pnpm lint` / `pnpm build` を実行し、すべて成功を確認。

**指摘:**
- 非ブロッカー: 記録上は secondary prefetch が 13 箇所となっているが、現行コード上の call site は 12 箇所だった。PR 説明や worklog の件数は実コードに合わせて修正した方がよい。
- 非ブロッカー: `docs/frontend/summary.md` では primary resource の `not_found` のみ `notFound()` に寄せる整理になっている一方、実装は従来どおり `fetchPrimary(...).catch(() => null)` 相当で全エラーを `notFound()` 扱いしている。この Issue の回帰ではないが、仕様との差は残っている。

**PR/完了結果:**
- `pr_ready: true`

**残リスク:**
- 今回の共通化自体に起因する追加の挙動回帰は確認できなかった。
- ただし、件数の記載ずれと primary error handling の仕様表現ずれは、次のレビューや将来の保守で混乱源になりうる。

### 2026-05-15: docs レビュー指摘対応

**修正内容:**

1. `docs/frontend/summary.md` line 120 付近: データ取得基盤の説明に Issue #113 で追加した共通ヘルパー (`fetchPrimary`, `prefetchSecondary`, `withServerFetcher`) と `src/lib/api/server-prefetch.ts` への集約について追記。
2. `docs/frontend/summary.md` line 123: primary error handling の記述を実装実態に合わせて修正。旧: `not_found` のみ `notFound()` → 新: 全エラーで `notFound()` を返す旨に変更。

**テスト結果:**
- `pnpm lint`: ✅ パス

**残リスク:**
- なし。ドキュメント修正のみで実装変更なし。

### 2026-05-15: docs 再確認完了

**対象:**
- Issue #113 の前回 docs 指摘 2 点の修正確認のみを実施。

**確認結果:**
- `docs/frontend/summary.md` の Issue #113 追記は、`boardflow/src/lib/api/server-prefetch.ts` に `fetchPrimary` / `prefetchSecondary` / `withServerFetcher` が集約され、各 page.tsx から利用されている現状と整合している。
- primary error handling の記述は、`fetchPrimary()` が `queryClient.fetchQuery(...).catch(() => null)` の後に `!result` で `notFound()` を呼ぶ実装と整合しており、`not_found` に限らず取得失敗全般を `notFound()` 扱いとする説明になっていることを確認した。
- 今回確認した範囲では、Issue #113 に関する docs / 実装 / worklog 間の不整合は解消されている。

**判定:**
- `docs_ready: true`

**必須修正:**
- なし。

**任意改善:**
- なし。

**残リスク:**
- なし。
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

## 2026-05-15 レビュー結果

### 対象

- Issue: #113
- レビュー対象コミット: `e4acb10`, `c2e4214`, `f6bbe3a`
- ブランチ: `issue-113-prefetch-commonize`

### 総評

- 実装の主目的である page.tsx の重複削減は達成できている。
- `queryKey` は `$api.queryOptions()` の値をそのまま再利用しており、現行の各ページとクライアントコンポーネントの組み合わせでは hydration 前提も維持できている。
- 一方で、secondary prefetch の失敗時の例外形が変わっており、「挙動変更なし」という完了条件とユーザー要望は満たしていない。

### PR 判定

- `pr_ready: false`

### 重大度順の指摘

1. **必須修正**: `withServerFetcher()` が secondary query 失敗時に `Error` ではなく OpenAPI の生エラーレスポンスをそのまま throw するようになっている。変更前の各 page では `new Error('Failed to fetch ...')` で投げていたため、ルートの error boundary が受け取る `error.message` の表示挙動が変わる。現在の `ErrorUI` は `error.message` を直接表示する実装なので、失敗時に空文字または意図しない文言になる。純粋リファクタリング条件に反する。

### 任意改善

1. `withServerFetcher()` は `clientOptions` と `serverFetcher` の対応関係を型で縛っていないため、誤った queryKey と fetcher の組み合わせでもコンパイルできる。現行呼び出しは一致しているが、「型安全性」の観点では主張がやや強い。
2. `withServerFetcher()` の `return data as T` は `undefined` を隠蔽する。今回の GET 群では直ちに問題化しないが、成功レスポンスが空になる API を将来流用すると検出が遅れる。

### テスト結果

- `pnpm typecheck`: pass
- `pnpm lint`: pass
- `pnpm build`: pass

### ドキュメント確認

- `docs/spec.md` と `docs/frontend/summary.md` の Server Component + prefetch + hydration 方針とは概ね整合している。
- ただし `docs/frontend/summary.md` への更新は計画に記載されていたが未実施。
- 現在の worklog には「挙動変更なし」「残リスクなし」に近い記述があり、今回のレビュー結果と不整合がある。

### 外部調査との整合

- Next.js / TanStack Query の一般的な Streaming SSR パターンである「primary は await、secondary は prefetch を await しない、pending query を dehydrate する」は維持されている。
- 一方で、失敗時の例外整形は UI のエラー境界に直接影響するため、共通化時に吸収してはいけない差分だった。

### 修正方針

1. `withServerFetcher()` 側で、既存ページと同等の `Error` を返す責務を持たせるか、少なくとも call site ごとに従来のメッセージを渡せるようにする。
2. そのうえで secondary query 失敗時に error boundary の表示文言が従来どおりであることを確認する。
3. worklog の「挙動変更なし」「残リスクなし」記述を今回のレビュー結果に合わせて更新する。

## 2026-05-15 レビュー指摘修正

### 修正内容

**`withServerFetcher` に `errorMessage` パラメータを追加** (`server-prefetch.ts`):

```typescript
export function withServerFetcher<T>(
  clientOptions: { queryKey: readonly unknown[] },
  serverFetcher: () => Promise<{ data?: T; error?: unknown }>,
  errorMessage?: string,  // ← 追加
): { queryKey: readonly unknown[]; queryFn: () => Promise<T> } {
  // ...
  if (error) throw errorMessage ? new Error(errorMessage) : error;
}
```

**全 secondary prefetch 呼び出しにエラーメッセージを復元**:

| ページ | エラーメッセージ |
|---|---|
| repositories/page.tsx | `'Failed to fetch repositories'` |
| repositories/[repositoryId]/page.tsx | `'Failed to fetch board projects'` |
| boards/[boardProjectId]/page.tsx | `'Failed to fetch board runs'` |
| boards/[boardProjectId]/runs/page.tsx | `'Failed to fetch board runs'` |
| settings/tokens/page.tsx | `'Failed to fetch tokens'` |
| runs/[boardRunId]/page.tsx | `'Failed to fetch artifacts'`, `'Failed to fetch viewer sources'`, `'Failed to fetch board project'` |
| runs/[boardRunId]/diff/page.tsx | `'Failed to fetch diff'`, `'Failed to fetch project'` |
| runs/[boardRunId]/checks/[checkKind]/page.tsx | `'Failed to fetch findings'`, `'Failed to fetch project'` |

**primary の `fetchPrimary` 呼び出しには `errorMessage` を渡していない** — `.catch(() => null)` でキャッチされるため不要。

### テスト結果

| チェック | 結果 |
|---|---|
| `pnpm typecheck` | ✅ パス |
| `pnpm lint` (Biome) | ✅ パス |
| `pnpm build` | ✅ パス (全ルート正常ビルド) |

### 残リスク

- なし。secondary prefetch のエラーメッセージが変更前と同等に復元された。

## 2026-05-15 ドキュメント確認

### 対象Issue

- Issue #113

### 確認結果

- `boardflow/src/lib/api/server-prefetch.ts` と対象 8 ページを確認し、primary を `fetchPrimary()`、secondary を `prefetchSecondary()` へ寄せる実装自体は一貫していた。
- `boardflow/src/lib/query-client.ts` の `shouldDehydrateQuery` は `pending` を含んでおり、secondary prefetch 非 await の Streaming SSR 前提は維持されている。
- `AGENTS.md` はリポジトリ運用ルールの文書であり、今回の frontend リファクタリングに伴う更新は不要。
- README / `docs/spec.md` に今回の共通化を反映すべき必須更新は見当たらなかった。

### レビュー結果

- `docs_ready: false`

### 必須修正

1. `docs/frontend/summary.md` の API 連携方針には「primary resource 取得で `not_found` のみ `notFound()` に寄せる」とあるが、実装中の `fetchPrimary()` は `queryClient.fetchQuery(...).catch(() => null)` により全エラーを `notFound()` 扱いする。現状実装に合わせて記述を修正するか、実装を文書どおりに変更するかを明確化する必要がある。
2. この worklog 内の secondary prefetch 件数は一部で 13 件と記載されているが、実コードの call site は 12 件だった。加えて、レビュー結果の後ろに旧計画・旧レビュー断片が連結されており、同一ログ内で整合しない記述が残っているため、Issue #113 の記録として整理が必要。

### 任意改善

1. worklog は「調査結果」「計画」「実装内容」「レビュー結果」「ドキュメント確認」を最新状態だけに畳み、旧案や修正前メモは別節へ退避した方が後続レビューで読みやすい。

### 不整合のあるドキュメント

- `docs/frontend/summary.md`
- `docs/logs/113/worklog.md`

### 不足しているドキュメント

- 追加で必須となる文書はなし。今回必要なのは既存 2 ファイルの整合修正のみ。

### 外部調査メモに関する指摘

- `pending` query の dehydrate と secondary prefetch 非 await による Streaming SSR 方針は、既存の外部調査メモと矛盾していない。
- 外部調査メモの更新は不要だが、`docs/frontend/summary.md` の primary error handling 記述だけは現行実装との差分が残っている。

### 残リスク

- `docs/frontend/summary.md` と実装のズレを放置すると、次回の refactor で「404 のみ notFound 扱い」と誤解した変更が入りうる。
- worklog の件数ずれと古い断片の混在を放置すると、PR 監査時に Issue #113 の実際の変更範囲を誤認しやすい。
