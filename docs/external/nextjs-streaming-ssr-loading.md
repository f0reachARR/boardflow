# Next.js 15 App Router Streaming SSR と loading.tsx

Issue: #65

## 要約

Next.js App Router は React Suspense をベースとした Streaming SSR をネイティブサポートする。`loading.tsx` によるページレベルのストリーミングと、手動 `<Suspense>` による粒度の細かいストリーミングの2つのアプローチがある。`loading.tsx` は該当ディレクトリの `page.tsx` を自動的に `<Suspense>` でラップする簡便な仕組み。

## 確認した情報

### 1. loading.tsx の仕組み

`loading.tsx` を `page.tsx` と同じディレクトリに配置すると、Next.js がそのページを自動的に `<Suspense>` でラップする。

```
app/
  dashboard/
    layout.tsx     ← すぐに表示される
    loading.tsx    ← page.tsx が読み込み中に表示される
    page.tsx       ← 非同期処理が完了すると表示
```

コンポーネント階層:

```
<Layout>
  <Suspense fallback={<Loading />}>
    <Page />
  </Suspense>
</Layout>
```

**重要な挙動:**
- `loading.tsx` は `layout.tsx` の内側、`page.tsx` の外側に配置される
- `layout.tsx` は `loading.tsx` でラップされない（layout はすぐに表示）
- ナビゲーション時に fallback UI がプリフェッチされるため即時表示
- デフォルトは Server Component だが `'use client'` を付けても可
- `not-found.tsx`, `page.tsx`, ネストされた `layout.tsx` を `<Suspense>` でラップする

### 2. 手動 `<Suspense>` による粒度制御

#### 並列ストリーミング（sibling boundaries）

```tsx
import { Suspense } from 'react'

export default function Dashboard() {
  return (
    <div>
      <h1>Dashboard</h1>
      <div className="grid grid-cols-2 gap-4">
        <Suspense fallback={<p>Loading revenue...</p>}>
          <Revenue />
        </Suspense>
        <Suspense fallback={<p>Loading orders...</p>}>
          <RecentOrders />
        </Suspense>
      </div>
    </div>
  )
}
```

各 `<Suspense>` 境界は独立してストリームする。Revenue が 200ms、Orders が 1s で解決される場合、Revenue が先に表示される。

#### ネストされた境界（progressive detail）

```tsx
<Suspense fallback={<p>Loading product details...</p>}>
  <ProductDetails id={id} />
  <Suspense fallback={<p>Loading reviews...</p>}>
    <Reviews productId={id} />
  </Suspense>
</Suspense>
```

外側が先に解決 → 内側の fallback が見える → 内側も解決という段階的な表示。

### 3. loading.tsx と `<Suspense>` の使い分け

| 観点 | `loading.tsx` | `<Suspense>` |
|---|---|---|
| スコープ | ページ全体 | 任意のコンポーネント |
| セットアップ | ファイル配置のみ | コンポーネントを明示的にラップ |
| ナビゲーション | fallback がプリフェッチされ即時表示 | デフォルトではプリフェッチされない |
| 適用場面 | データなしでは何も表示できないページ | ほとんどのページ。粒度の細かい制御向き |

**公式推奨**: 明示的な `<Suspense>` 境界を動的データアクセスの近くに配置することを推奨。`loading.tsx` をツリーの上位に置くとページ全体がスケルトンになり、粒度が粗くなる。

### 4. 動的アクセスを下位に押し下げるパターン

```tsx
export default function DashboardLayout({ children }) {
  const cookieStore = cookies() // await しない
  return (
    <div>
      <Nav>
        <Suspense fallback={<p>Loading user...</p>}>
          <UserMenu cookiePromise={cookieStore} />
        </Suspense>
      </Nav>
      {children}
    </div>
  )
}
```

`cookies()` を `await` せずに Promise のまま渡すことで、layout の他の部分は static shell として即座にレンダリングされる。

### 5. サーバーからクライアントへのデータストリーミング

Server Component で fetch を開始し、未解決の Promise を Client Component に渡す:

```tsx
// Server Component (page.tsx)
export default function Dashboard() {
  const statsPromise = getStats() // await しない
  return (
    <Suspense fallback={<p>Loading chart...</p>}>
      <StatsChart dataPromise={statsPromise} />
    </Suspense>
  )
}

// Client Component
'use client'
import { use } from 'react'
export function StatsChart({ dataPromise }) {
  const stats = use(dataPromise)
  return <div>{/* render chart */}</div>
}
```

### 6. エラーハンドリング

- ストリーミング開始後にコンポーネントがエラーをスローすると、最も近い `error.tsx` 境界がキャッチ
- 失敗したセクションだけがエラー UI に置き換わり、残りのページは影響を受けない
- HTTP ステータスコードは `200 OK` のまま変更不可（ヘッダーは最初のチャンクで送信済み）

### 7. SEO への影響

- ストリーミングはサーバーレンダリングなので SEO に影響しない
- HTML限定のボット（Twitterbot等）には `generateMetadata` がストリーミング前に解決される
- フルブラウザ対応のクローラーにはメタデータもストリーミング可能
- `notFound()` がミッドストリームで呼ばれた場合、`<meta name="robots" content="noindex">` が注入される

### 8. Web Vitals への効果

| 指標 | 効果 |
|---|---|
| TTFB | 最も遅いクエリではなく、static shell の生成時間まで短縮 |
| FCP | ブラウザが static shell を即座に描画 |
| LCP | LCP 要素を Suspense 境界の外側に配置すれば高速 |
| CLS | スケルトン fallback の寸法を実コンテンツと合わせれば回避可能 |
| INP | 選択的ハイドレーションにより、各 Suspense 境界が独立してハイドレート |

### 9. インフラ考慮事項

- リバースプロキシ（Nginx等）: `X-Accel-Buffering: no` ヘッダーが必要
- CDN: チャンクドレスポンスのパススルー設定が必要な場合あり
- サーバレス: AWS Lambda は response streaming mode の明示的有効化が必要
- 圧縮: gzip/brotli がチャンクをバッファリングする場合あり
- Safari/WebKit: 1024バイト未満のレスポンスをバッファリング

## BoardFlow への示唆

### 推奨戦略

1. **`loading.tsx` を各ルートセグメントに配置**: 最低限のローディングUI保証として
   - `app/repositories/loading.tsx` — リポジトリ一覧用
   - `app/repositories/[repositoryId]/loading.tsx` — リポジトリ詳細用
   - `app/repositories/[repositoryId]/boards/[boardProjectId]/loading.tsx`
   - `app/repositories/[repositoryId]/boards/[boardProjectId]/runs/loading.tsx`
   - `app/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/loading.tsx`

2. **粒度の細かい `<Suspense>` を追加**: ページ内で独立して読み込めるセクションに
   - ラン詳細ページ: ヘッダー情報 / artifact グリッド / diff セクションを独立した Suspense 境界に
   - ボードプロジェクト詳細: プロジェクト情報 / 最近のラン一覧を独立に

3. **TanStack Query の prefetch (await なし) + useSuspenseQuery パターンとの組み合わせ**:
   - Server Component で `queryClient.prefetchQuery(...)` を await せずに呼ぶ
   - Client Component で `useSuspenseQuery` を使う
   - `<Suspense>` の fallback にスケルトン UI を配置

### VPS デプロイでの考慮

- BoardFlow は VPS デプロイ（Node.js サーバー）のため、ストリーミングはネイティブサポート
- Nginx をリバースプロキシに使う場合は `X-Accel-Buffering: no` の設定が必要

## 採用/不採用判断

**採用**: Next.js App Router の標準機能であり、追加パッケージ不要。`loading.tsx` は最小限のボイラープレートで即座に効果が得られる。

## 制約と pitfall

1. **layout での `await`**: layout で `cookies()` や `headers()` を `await` すると、`loading.tsx` の fallback が表示されない。動的アクセスは下位コンポーネントに押し下げるか `<Suspense>` でラップする
2. **HTTP ステータスコード**: ストリーミング開始後はステータスコード変更不可。404 を正しく返すには Suspense 境界の前で `notFound()` を呼ぶ
3. **CLS リスク**: スケルトンと実コンテンツの寸法が異なるとレイアウトシフトが発生
4. **LCP 要素の配置**: LCP 要素（メインヘッダーなど）を Suspense 境界の外側に置かないと LCP が遅延
5. **ブラウザバッファリング**: Safari は 1024 バイト未満のレスポンスをバッファリングするが、実アプリでは通常問題にならない

## 参照URL

- https://nextjs.org/docs/app/api-reference/file-conventions/loading （loading.js API リファレンス）
- https://nextjs.org/docs/app/guides/streaming （Next.js Streaming ガイド — 2026-04-10 更新）
