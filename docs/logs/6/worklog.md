# Issue #6: Web UI Read API実装

## 経緯
- バックエンド実装Issue分割タスクの一環として作成
- フロントエンドが利用するRead API群

## ユーザー要望
- docs/以下の仕様に基づくRustバックエンド実装の第6段階

## Issue作成内容
- Repository/BoardProject/BoardRun/Artifact 一覧・詳細 + Viewer Sources API
- URL: https://github.com/f0reachARR/boardflow/issues/6

## 後続処理タイプの初期仮説
`implementation_required`

## 残リスク
- GitHub OAuth session 認証のMVP初期対応方針
- cursor pagination の encode/decode 実装パターン
