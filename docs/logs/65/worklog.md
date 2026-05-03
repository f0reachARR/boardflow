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
