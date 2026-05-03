# Issue #63: GitHub APIレスポンスのDBキャッシュとレートリミット対策

## Issueまでの経緯

- crates/api/src/routes/read.rs の list_repositories がリクエストごとにGitHub API呼び出し
- crates/api/src/github_access.rs の RealGithubAccessChecker が毎回APIコール
- 現在キャッシュ層は一切存在しない
- GitHub APIレートリミットは5000 req/h per user token
- 既存の関連Issueなし

## ユーザー要望

レポジトリ一覧APIなど一覧系APIで毎回GitHub APIを呼び出しているため、レートリミットに引っかかる可能性がある。適切なinvalidateの実装とともにDBにキャッシュして、GitHub APIの呼び出し回数を減らす必要がある。

## Issue作成内容

- Issue #63として新規作成
- labels: backend, api
- インメモリ/DBキャッシュ導入、TTL設定、invalidate戦略、429フォールバック

## 後続処理タイプの初期仮説

`implementation_required`

## 残リスク

- キャッシュの陳腐化: リポジトリ権限変更がリアルタイム反映されない
- インメモリキャッシュはマルチインスタンスデプロイ時に各ノードで不整合
- GitHub App installation_repositories webhookのinvalidateタイミング設計
