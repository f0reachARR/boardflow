# Issue #90 - action-runner APIクライアントを共有型(api-types)ベースにリファクタリングする

## 経緯

- Issue #89 で共有crate作成後、action-runner側のリファクタリングが必要

## ユーザー要望

- action-runnerのAPI呼び出しを型安全にする
- 手動のjsonマクロ構築を廃止する

## Issue作成内容

`action-runner/src/api.rs` の独自型定義を削除し、`api-types` crateの型を使用。
リクエスト構築を構造体ベースに変更。

## 後続処理タイプ

`implementation_required`

## 残リスク

- wiremockテストのレスポンスも更新が必要
- `runner.rs` でのペイロード構築ロジックの大規模変更
