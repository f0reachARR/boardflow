# Issue #2: DBマイグレーション・データモデル実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- spec.md Section 10 の全テーブルをマイグレーション化

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第2段階

## Issue作成内容
- 13テーブルのSQLxマイグレーション作成
- URL: https://github.com/f0reachARR/boardflow/issues/2

## 後続処理タイプの初期仮説
`implementation_required`

## 残リスク
- PostgreSQL enum vs CHECK 制約の選択が未決定
- JSONB index の要否はMVP後に判断
