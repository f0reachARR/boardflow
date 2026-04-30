# Issue #4: Action API: Plan API実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- Action→SaaS の最初の呼び出しポイント

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第4段階

## Issue作成内容
- POST /api/v1/runs/plan の完全実装（差分判定含む）
- URL: https://github.com/f0reachARR/boardflow/issues/4

## 後続処理タイプの初期仮説
`implementation_required`

## 残リスク
- utoipa-axum での複雑な request body 定義方法
