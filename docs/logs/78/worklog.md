# Issue #78: TanStack Queryへのデータフェッチ全面リファクタリング

## 経緯
- ユーザー要望7: fetchやclient.GETなどをTanStack Queryに移行
- Issue #64 (CLOSED) でTanStack Query基盤は導入済み、しかし多くのページが旧パターンのまま

## ユーザー要望
- フロントエンドの全データフェッチをTanStack Query (`$api`) に統一

## 調査結果
- 基盤: `openapi-react-query` の `$api` クライアント、`QueryClientProvider`、`getQueryClient()` が設定済み
- リポジトリ一覧ページのみ `prefetchQuery` + `HydrationBoundary` パターンを適用済み
- 他の多数のページが `client.GET()` 直接呼び出しのまま
- `create-token-dialog.tsx` の mutation も `apiClient.POST` 直接呼び出し

## Issue作成内容
- タイトル: TanStack Queryへのデータフェッチ全面リファクタリング
- ラベル: enhancement, frontend
- 新規作成

## 後続処理タイプ
`implementation_required`

## 残リスク
- ページ数が多く段階的移行が必要
- Server Component での prefetch パターンの統一
