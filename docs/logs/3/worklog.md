# Issue #3: 認証基盤とAPI Token

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- 全Action APIの横断的関心事（認証、エラー形式）

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第3段階

## Issue作成内容
- Bearer token認証、統一エラーレスポンス、リクエストID
- URL: https://github.com/f0reachARR/boardflow/issues/3

## 後続処理タイプの初期仮説
`implementation_required`

## 残リスク
- token hash アルゴリズム（SHA-256 vs argon2）未決定
- Axum middleware vs extractor の選択
