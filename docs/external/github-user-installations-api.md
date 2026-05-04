# GitHub User Installations API — リポジトリ同期フォールバック調査

## 要約

GitHub App Webhookが不着の場合にユーザーのリポジトリ一覧をDBに同期するために、ユーザーのOAuthトークンで `GET /user/installations` → `GET /user/installations/{installation_id}/repositories` を呼ぶフォールバック戦略を調査した。両APIともGitHub App user access tokenで利用可能であり、BoardFlow の既存 `repositories` テーブルへの upsert に必要な情報（`id`, `full_name`, installation の `id`）がすべてレスポンスに含まれる。

## 確認した情報

### 1. GET /user/installations — ユーザーがアクセスできるインストール一覧

**エンドポイント**: `GET /user/installations`

**認証**: GitHub App user access token（Bearer token）。追加のパーミッション不要。

**説明**: 認証済みユーザーが明示的なパーミッション（:read, :write, :admin）を持つ GitHub App インストールの一覧を返す。ユーザーは以下のリポジトリにアクセスできる:
- 自分が所有するリポジトリ
- コラボレーターとして参加しているリポジトリ
- Organization メンバーシップでアクセス可能なリポジトリ

**クエリパラメータ**:

| パラメータ | 型 | デフォルト | 説明 |
|---|---|---|---|
| `per_page` | integer | 30 | ページあたりの結果数（最大100） |
| `page` | integer | 1 | フェッチするページ番号 |

**レスポンス形式** (200):

```json
{
  "total_count": 2,
  "installations": [
    {
      "id": 1,
      "account": {
        "login": "octocat",
        "id": 1,
        "type": "User"
      },
      "app_id": 1,
      "target_id": 1,
      "target_type": "Organization",
      "permissions": {
        "checks": "write",
        "metadata": "read",
        "contents": "read"
      },
      "events": ["push", "pull_request"],
      "repository_selection": "all",
      "created_at": "2017-07-08T16:18:44-04:00",
      "updated_at": "2017-07-08T16:18:44-04:00",
      "app_slug": "github-actions",
      "suspended_at": null,
      "suspended_by": null
    }
  ]
}
```

**エラーレスポンス**:

| ステータス | 説明 |
|---|---|
| 200 | 成功 |
| 304 | Not Modified（ETag/Last-Modified キャッシュ） |
| 401 | 認証が必要（トークン無効/期限切れ） |
| 403 | 禁止（レートリミット超過を含む） |

**重要なフィールド**:
- `installations[].id` — installation_id。次の API 呼び出しに使用
- `installations[].account.login` — インストール先のアカウント名
- `installations[].repository_selection` — `"all"` または `"selected"`（全リポジトリ or 選択リポジトリ）
- `installations[].app_id` — BoardFlow の App ID と一致するものだけ処理すべき
- `installations[].suspended_at` — null でなければサスペンド中

### 2. GET /user/installations/{installation_id}/repositories — インストール経由でアクセス可能なリポジトリ一覧

**エンドポイント**: `GET /user/installations/{installation_id}/repositories`

**認証**: GitHub App user access token（Bearer token）。`metadata` リポジトリパーミッション（read）が必要。

**説明**: 指定インストールに対してユーザーが明示的パーミッションを持つリポジトリの一覧を返す。

**パスパラメータ**:

| パラメータ | 型 | 説明 |
|---|---|---|
| `installation_id` | integer | インストールの一意識別子（必須） |

**クエリパラメータ**:

| パラメータ | 型 | デフォルト | 説明 |
|---|---|---|---|
| `per_page` | integer | 30 | ページあたりの結果数（最大100） |
| `page` | integer | 1 | フェッチするページ番号 |

**レスポンス形式** (200):

```json
{
  "total_count": 1,
  "repositories": [
    {
      "id": 1296269,
      "node_id": "MDEwOlJlcG9zaXRvcnkxMjk2MjY5",
      "name": "Hello-World",
      "full_name": "octocat/Hello-World",
      "owner": {
        "login": "octocat",
        "id": 1
      },
      "private": false,
      "html_url": "https://github.com/octocat/Hello-World",
      "visibility": "public",
      "permissions": {
        "admin": false,
        "push": false,
        "pull": true
      }
    }
  ]
}
```

**エラーレスポンス**:

| ステータス | 説明 |
|---|---|
| 200 | 成功 |
| 304 | Not Modified |
| 403 | 禁止 |
| 404 | インストールが見つからない、またはアクセス権なし |

**重要なフィールド**:
- `repositories[].id` — GitHub repository ID（`github_repository_id` として DB に保存）
- `repositories[].full_name` — `"owner/name"` 形式。`split_once('/')` で owner と name を分離可能
- `repositories[].owner.login` — リポジトリオーナー名
- `repositories[].name` — リポジトリ名

### 3. ページネーション

両APIとも標準的な GitHub ページネーション:
- `per_page` 最大100、デフォルト30
- `page` パラメータでページ指定
- `Link` レスポンスヘッダに `rel="next"`, `rel="last"` が含まれる
- `total_count` フィールドで総数を事前取得可能

**BoardFlow 実装**: `per_page=100` を指定し、`total_count` と取得済み件数を比較してループするか、`repos.len() < 100` で打ち切る（既存の `list_accessible_repo_ids` と同じパターン）。

### 4. レートリミット

| 認証方式 | 制限 |
|---|---|
| User access token（OAuth） | 5,000 req/h（ユーザー単位で共有） |
| GitHub Enterprise Cloud | 15,000 req/h |

**API コール消費の見積もり**:
- `GET /user/installations`: 1回（通常1ページで収まる。BoardFlow App のインストールは多くて数個）
- `GET /user/installations/{id}/repositories`: インストールごとに ceil(repos / 100) 回
- 典型的なケース: 1 installation × 10 repos = **2 API コール**
- 最悪ケース: 3 installations × 500 repos each = **18 API コール**

BoardFlow は既に `list_accessible_repo_ids` で `/user/repos` を全ページ取得しており、そちらのコストのほうが大きい。フォールバック同期が追加するコストは限定的。

### 5. `GET /user/repos` との違い

| 特性 | `/user/repos` | `/user/installations/{id}/repositories` |
|---|---|---|
| 範囲 | ユーザーがアクセスできる全リポジトリ | 特定 installation に紐づくリポジトリのみ |
| installation_id | 取得不可 | パスパラメータから確定 |
| 用途 | アクセス可能リポジトリの権限チェック | Webhook 不着時のリポジトリ同期 |

**重要**: `/user/repos` のレスポンスには `installation_id` が含まれない。`repositories` テーブルの `installation_id` カラムを埋めるには、`/user/installations/{id}/repositories` を使う必要がある。

## BoardFlow への示唆

### フォールバック同期の発火タイミング

`CachedGithubAccessChecker::list_accessible_repo_ids` のフロー内で、GitHub API から返った `repo_id` 群と DB の `repositories` テーブルを比較し、DB に存在しない `repo_id` が見つかった場合にフォールバック同期を発火する設計が妥当。

**提案フロー**:
1. 既存通り `/user/repos` で `accessible_repo_ids: Vec<i64>` を取得
2. DB から `SELECT github_repository_id FROM repositories WHERE github_repository_id = ANY($1)` で既存 ID を取得
3. 差分（DB に未登録の ID）が存在する場合:
   a. `GET /user/installations?per_page=100` で BoardFlow App のインストール一覧を取得
   b. 各 installation_id に対して `GET /user/installations/{id}/repositories?per_page=100` で repos を取得
   c. 取得した repos を `boardflow_db::queries::repository::upsert` で DB に同期
4. 差分がなければ同期スキップ

### キャッシュ/スロットル設計

- 同期頻度制限: `github_api_cache` テーブルに `cache_type = "installation_repos_sync"` として最終同期時刻を記録、TTL 10分
- TTL 内は差分があってもフォールバック同期をスキップ
- ユーザーごとに独立したスロットル（既存の `user_id` + `cache_type` の複合キー設計を活用）

### app_id フィルタリング

`GET /user/installations` は該当ユーザーがアクセスできる**すべての** GitHub App のインストールを返す。BoardFlow 以外の App のインストールも含まれるため、`installations[].app_id` が BoardFlow の App ID と一致するものだけを処理すべき。

### suspended インストールの除外

`installations[].suspended_at` が null でないインストールはサスペンド中であり、リポジトリ取得がエラーになる可能性がある。フィルタリングで除外すべき。

## 採用/不採用判断

**採用**: `GET /user/installations` + `GET /user/installations/{installation_id}/repositories` によるフォールバック同期

**理由**:
1. ユーザーの OAuth トークンで呼べるため、追加の認証情報が不要
2. `installation_id` を含むリポジトリ情報が取得でき、既存の upsert 関数をそのまま使える
3. API コール数が少なく、レートリミットへの影響が限定的
4. 既存の `github_api_cache` テーブルと `CachedGithubAccessChecker` パターンに自然に統合できる

## 制約と pitfall

1. **レートリミット共有**: user access token のレートリミット（5,000 req/h）は同ユーザーの他のアプリ・PATと共有される。フォールバック同期が他のAPI呼び出しを圧迫しないよう、スロットルが必須
2. **app_id フィルタ**: `/user/installations` は全 App のインストールを返すため、BoardFlow の app_id 以外を必ずフィルタする
3. **repository_selection=all**: インストールが "all" の場合、org 内の全リポジトリが返される可能性がある。大量リポジトリの org ではページネーションコストが増加する
4. **404 の意味**: `/user/installations/{id}/repositories` で 404 が返る場合、そのインストールへのアクセス権がないか、インストールが削除されている。エラーハンドリングが必要
5. **最終的整合性**: Webhook 到着と API 結果の間にタイムラグがある可能性がある。新規インストール直後は API にまだ反映されていない場合がある
6. **full_name のパース**: `repositories[].full_name` の `split_once('/')` が失敗するケースは実質ないが、既存の webhook handler と同様にガード処理を入れるべき

## 未解決の疑問

1. BoardFlow の `app_id` をどこから取得するか（環境変数 or DBの設定テーブル）。現在の `crates/github/` のコンフィグ構造を確認する必要がある
2. フォールバック同期を `CachedGithubAccessChecker` 内で直接行うか、別の `RepositorySyncService` に分離するか。前者は責務が増えるが、フロー制御が簡潔
3. 同期処理を API リクエストのホットパスで行うか、バックグラウンドジョブとしてキューに投入するか。ユーザー体験としてはホットパスのほうが即時反映されるが、レイテンシが増加する

## 参照URL

- https://docs.github.com/en/rest/apps/installations?apiVersion=2022-11-28#list-app-installations-accessible-to-the-user-access-token
- https://docs.github.com/en/rest/apps/installations?apiVersion=2022-11-28#list-repositories-accessible-to-the-user-access-token
- https://docs.github.com/en/rest/apps/installations?apiVersion=2022-11-28#list-repositories-accessible-to-the-app-installation
- https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api
