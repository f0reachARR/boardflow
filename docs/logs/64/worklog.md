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

## 残リスク

- openapi-fetchとTanStack Queryの統合パターンの設計判断
- Server ComponentでのprefetchとClient Componentでの再利用の複雑性
- 既存ページへの段階的移行計画
