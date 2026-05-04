# Issue #77: Webhook不着時のリポジトリ一覧取得: Installed Repositories APIフォールバック

## 経緯
- ユーザー要望4: Webhookが不着の場合にリポジトリ一覧が表示されない

## ユーザー要望
- GitHub Appインストール直後やWebhook不着時にもリポジトリ一覧を表示したい

## 調査結果
- 現在: Webhook (`installation`/`installation_repositories` イベント) 経由でのみDBにリポジトリが登録される
- `routes/webhook.rs`: `handle_installation_event` / `handle_installation_repositories_event` でupsert
- GitHub API: `GET /installation/repositories` (Installation Token), `GET /user/installations/{id}/repositories` (User Token)
- `CachedGithubAccessChecker` が既存のキャッシュ機構として存在

## Issue作成内容
- タイトル: Webhook不着時のリポジトリ一覧取得: Installed Repositories APIフォールバック
- ラベル: bug, backend, api
- 新規作成

## 後続処理タイプ
`implementation_required`

## 残リスク
- レートリミットへの影響
- ユーザーの Installation へのアクセス権限の正確な判定
