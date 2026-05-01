# GitHub App 認証クライアント実装調査 (octocrab)

## 要約

octocrab crate (v0.49.9) は GitHub App 認証（JWT 生成 + Installation Token 取得）と Issue 操作（作成・コメント作成・コメント編集）を **すべてネイティブにサポート** している。
octocrab 内部で `jsonwebtoken` crate を利用しており、RS256 JWT 生成を自前で書く必要がない。
BoardFlow の `crates/github/` に octocrab を採用することで、GitHub App 認証と Issue 連携を最小限のコードで実装できる。

## 確認した情報

### 1. octocrab の GitHub App 認証サポート

**結論: 完全サポートされている**

- `OctocrabBuilder::app(app_id, key)` で GitHub App として認証する Octocrab インスタンスを構築できる
- `octocrab::auth::create_jwt(app_id, &key)` で JWT を生成できる（内部で jsonwebtoken v10 + RS256 を利用）
- `Octocrab::installation(installation_id)` で Installation Token を自動取得し、installation スコープの Octocrab を得られる
- `Octocrab::installation_and_token(installation_id)` で Installation Token の値も取得可能
- `Octocrab::installation_token()` で 30 秒以上有効なキャッシュ済みトークンを取得可能
- `Octocrab::installation_token_with_buffer(duration)` でカスタムバッファ付きトークン取得が可能

**内部 JWT 生成の実装:**

octocrab の `auth.rs` 内で以下のように JWT を生成している：

```rust
// octocrab 内部実装 (src/auth.rs)
#[derive(Serialize)]
struct Claims {
    iss: AppId,
    iat: usize,
    exp: usize,
}

let now = SystemTime::UNIX_EPOCH.elapsed().unwrap().as_secs() as usize;
let claims = Claims {
    iss: github_app_id,
    iat: now - 60,      // 60秒前（clock drift対策）
    exp: now + (9 * 60), // 9分後（GitHub上限10分以内）
};
let header = Header::new(Algorithm::RS256);
jsonwebtoken::encode(&header, &claims, key)
```

### 2. octocrab の Issue 操作サポート

**結論: BoardFlow で必要な操作はすべてサポートされている**

| 操作 | octocrab メソッド | BoardFlow 用途 |
|---|---|---|
| Issue 作成 | `issues(owner, repo).create(title).body(body).send().await` | BoardProject 初回 completed 後の Issue 自動作成 |
| Issue 取得 | `issues(owner, repo).get(number).await` | Issue 存在確認、状態確認 |
| Issue 更新 | `issues(owner, repo).update(number).title(...).body(...).state(...).send().await` | タイトル・状態更新 |
| Comment 作成 | `issues(owner, repo).create_comment(number, body).await` | Dashboard / Run Result コメント作成 |
| Comment 取得 | `issues(owner, repo).get_comment(comment_id).await` | コメント存在確認 |
| Comment 更新 | `issues(owner, repo).update_comment(comment_id, body).await` | Dashboard コメント編集更新 |
| Comment 削除 | `issues(owner, repo).delete_comment(comment_id).await` | （将来用） |
| Comment 一覧 | `issues(owner, repo).list_comments(number).send().await` | コメント検索 |

### 3. octocrab の依存関係

octocrab v0.49.9 の主要依存:

| crate | バージョン | 用途 |
|---|---|---|
| jsonwebtoken | 10 | RS256 JWT 生成（`use_pem` feature） |
| secrecy | 0.10.3 | 秘密情報の安全な管理 |
| hyper | 1.1.0 | HTTP クライアント |
| hyper-rustls | 0.27.0 (optional) | TLS |
| serde / serde_json | 1.x | シリアライズ |
| chrono | 0.4 | 日時処理 |
| tokio | 1.x (optional) | 非同期ランタイム |

**重要な feature flags:**

- `default` = `["follow-redirect", "retry", "rustls", "timeout", "tracing", "default-client", "rustls-ring", "jwt-rust-crypto"]`
- `jwt-rust-crypto` — jsonwebtoken の `rust_crypto` backend を使用（デフォルト）
- `jwt-aws-lc-rs` — jsonwebtoken の `aws_lc_rs` backend を使用（`jwt-rust-crypto` と排他）
- `stream` — ページネーション用のストリームサポート

### 4. RS256 JWT 生成（GitHub App 用）

**GitHub 公式要件:**

| Claim | 意味 | 値 |
|---|---|---|
| `iss` | 発行者 | GitHub App の Client ID（推奨）または App ID |
| `iat` | 発行時刻 | 現在時刻 - 60秒（clock drift 対策） |
| `exp` | 有効期限 | 現在時刻 + 最大10分 |
| `alg` | アルゴリズム | RS256（必須） |

- JWT の最大有効期間は **10分**
- octocrab は `iat = now - 60`, `exp = now + 9分` を使用
- 秘密鍵は GitHub App 設定ページからダウンロードした PEM 形式

### 5. GitHub App Installation Token API

**エンドポイント:** `POST /app/installations/{installation_id}/access_tokens`

**認証:** JWT を `Authorization: Bearer <JWT>` ヘッダで送信

**リクエストボディ（省略可）:**

```json
{
  "repositories": ["repo-name"],
  "repository_ids": [123],
  "permissions": {
    "issues": "write",
    "metadata": "read"
  }
}
```

**レスポンス:**

```json
{
  "token": "ghs_xxxx",
  "expires_at": "2024-01-01T01:00:00Z",
  "permissions": {
    "issues": "write",
    "metadata": "read"
  },
  "repository_selection": "all"
}
```

- Token 有効期限: **1時間**
- permissions 未指定時は App に付与された全権限を継承
- repositories/repository_ids 未指定時は Installation がアクセス可能な全リポジトリ

**octocrab での利用:** `installation()` / `installation_and_token()` がこの API を内部で呼び出す。トークンキャッシュも内蔵。

## BoardFlow への示唆

### octocrab が BoardFlow の要件をカバーする範囲

1. **GitHub App JWT 認証** — `OctocrabBuilder::app()` で完全対応
2. **Installation Token 取得** — `installation()` + 自動キャッシュ
3. **Issue 作成** — `issues().create()` で対応
4. **Comment 作成** — `issues().create_comment()` で対応
5. **Comment 編集** — `issues().update_comment()` で対応
6. **Issue 取得・状態確認** — `issues().get()` で対応
7. **Issue 更新（close/reopen）** — `issues().update().state()` で対応

### crates/github/ の設計方針

octocrab を直接使うのではなく、BoardFlow 固有のトレイト（例: `GitHubAppClient`）を定義し、octocrab をその実装として注入する構成を推奨：

```rust
// crates/github/src/lib.rs
#[async_trait::async_trait]
pub trait GitHubAppClient: Send + Sync {
    async fn create_issue(&self, owner: &str, repo: &str, title: &str, body: &str) -> Result<Issue>;
    async fn create_comment(&self, owner: &str, repo: &str, issue_number: u64, body: &str) -> Result<Comment>;
    async fn update_comment(&self, owner: &str, repo: &str, comment_id: u64, body: &str) -> Result<Comment>;
    async fn get_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Option<Issue>>;
    // ...
}
```

理由:
- テスト時にモック差し替え可能
- octocrab の型を domain/worker に漏らさない
- 既存の `GithubAccessChecker` トレイト（`crates/api/src/github_access.rs`）と同じパターン

## 採用/不採用判断

### 推奨: octocrab を採用

**理由:**

1. **JWT 生成が組み込み**: `jsonwebtoken` crate を内部利用しており、自前実装不要
2. **Installation Token 取得・キャッシュが組み込み**: 1時間有効なトークンの自動管理
3. **型安全な Issue API**: `IssueHandler` で Issue/Comment の CRUD を型安全に実行可能
4. **活発にメンテナンス**: v0.49.9（2025年時点で最新）、High reputation
5. **依存関係の重複が少ない**: 既存プロジェクトで使用中の `serde`, `tokio`, `chrono`, `reqwest`（内部では hyper だが互換）と共存可能
6. **秘密情報管理**: `secrecy::SecretString` による安全なトークン管理

**reqwest + jsonwebtoken で自前実装する場合との比較:**

| 観点 | octocrab | reqwest + jsonwebtoken |
|---|---|---|
| JWT 生成 | 組み込み | 自前実装必要（~20行） |
| Installation Token | 自動取得・キャッシュ | 自前実装必要（~50行 + キャッシュロジック） |
| Issue API | 型安全なビルダー | 自前で JSON 構築（~100行） |
| Comment API | 型安全メソッド | 自前で JSON 構築（~60行） |
| エラーハンドリング | 構造化エラー型 | 自前定義必要 |
| レスポンス型 | 充実したモデル | 自前定義必要 |
| 依存の重さ | やや重い（hyper + tower） | 軽い（reqwest のみ追加） |
| メンテナンス負担 | 低い | 高い（GitHub API 変更時に追従） |

**判定: octocrab 採用を推奨。** 自前実装の利点（依存削減）より、型安全性・メンテナンス負担軽減の利点が大きい。

### Cargo.toml に追加すべき依存

**ワークスペースルート Cargo.toml:**

```toml
[workspace.dependencies]
octocrab = "0.49"
jsonwebtoken = { version = "10", default-features = false, features = ["use_pem"] }
secrecy = "0.10"
```

**crates/github/Cargo.toml:**

```toml
[dependencies]
octocrab = { workspace = true }
jsonwebtoken = { workspace = true }  # EncodingKey 型の直接利用が必要な場合
secrecy = { workspace = true }       # SecretString 型の利用
tokio = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
```

注: `jsonwebtoken` は octocrab が内部で利用しているが、`EncodingKey::from_rsa_pem()` を直接呼ぶ場合は明示的な依存が必要。octocrab は `pub use jsonwebtoken::EncodingKey` を re-export しているので、octocrab 経由でアクセスすることも可能。

## 制約と pitfall

1. **octocrab の hyper vs プロジェクトの reqwest**: octocrab は内部で hyper を使い、BoardFlow は reqwest を使っている。両方が依存ツリーに含まれるが、直接の衝突はない（reqwest も内部で hyper を使用）。バイナリサイズはやや増加する。

2. **Installation Token のキャッシュスコープ**: `Octocrab::installation()` で得たインスタンスはトークンをキャッシュするが、インスタンスのライフタイムに紐づく。worker ジョブごとにインスタンスを作り直す場合、毎回トークン取得が発生する。`installation_and_token()` でトークンを外部キャッシュに保存する設計も検討すべき。

3. **Rate Limit**: octocrab 自体はレートリミット管理を提供しない。BoardFlow の要件（installation_id 単位・repository_id 単位の並列数制御）は自前で実装する必要がある。

4. **GitHub App の iss claim**: GitHub は 2024 年以降、`iss` に App ID ではなく **Client ID** の使用を推奨している。octocrab の `create_jwt` は `AppId` 型を使用するが、Client ID を数値に変換して渡す必要がある場合は注意。octocrab の `AppId` は `u64` のラッパーなので、Client ID が文字列の場合は octocrab の `create_jwt` を使わず自前で JWT を生成するか、App ID（数値）を使い続ける。

5. **secrecy crate のバージョン**: octocrab は secrecy 0.10.3 を使用。BoardFlow の他の crate で異なるバージョンの secrecy を使うと衝突する可能性がある。

6. **octocrab の Edition**: octocrab は Rust Edition 2018 を使用。BoardFlow は Edition 2024 を使用しているが、依存関係としては問題ない。

7. **Feature flag 選択**: デフォルト features にはテストで不要なものも含まれる。最小構成で使いたい場合は `default-features = false` にして必要な features のみ有効化する。ただし、`jwt-rust-crypto` は必須。

## コード例

### GitHub App 認証と Installation Token 取得

```rust
use octocrab::{Octocrab, models::AppId};
use jsonwebtoken::EncodingKey;
use secrecy::SecretString;

// PEM 形式の秘密鍵から EncodingKey を生成
let pem = std::fs::read("path/to/private-key.pem")?;
let key = EncodingKey::from_rsa_pem(&pem)?;

// GitHub App として認証
let app_id = AppId(12345);
let octocrab = Octocrab::builder()
    .app(app_id, key)
    .build()?;

// Installation Token を取得して installation スコープの Octocrab を得る
let installation_id = octocrab::models::InstallationId(67890);
let (installation_crab, token) = octocrab
    .installation_and_token(installation_id)
    .await?;

// この installation_crab で API 操作が可能
```

### Issue 作成

```rust
let issue = installation_crab
    .issues("owner", "repo")
    .create("[Board] motor_driver")
    .body("<!-- boardflow:repository_id=123 -->\n# Board Project\n...")
    .send()
    .await?;

let issue_number = issue.number;
let issue_id = issue.id;  // CommentId 等で使える
```

### Comment 作成

```rust
let comment = installation_crab
    .issues("owner", "repo")
    .create_comment(issue_number, "<!-- boardflow:comment_type=dashboard -->\n## BoardFlow Dashboard\n...")
    .await?;

let comment_id = comment.id;  // 後で update_comment に使う
```

### Comment 編集

```rust
let updated_comment = installation_crab
    .issues("owner", "repo")
    .update_comment(comment_id, "<!-- boardflow:comment_type=dashboard -->\n## Updated Dashboard\n...")
    .await?;
```

### Issue 取得・状態確認

```rust
use octocrab::models::IssueState;

let issue = installation_crab
    .issues("owner", "repo")
    .get(issue_number)
    .await?;

match issue.state {
    IssueState::Open => { /* Issue is open */ }
    IssueState::Closed => { /* Issue is closed, check recreate_issue_on_update */ }
    _ => {}
}
```

## 未解決の疑問

1. **octocrab の `AppId` と GitHub Client ID の関係**: GitHub は `iss` に Client ID（文字列）の使用を推奨しているが、octocrab は `AppId(u64)` を使用している。App ID（数値）での運用で問題ないか、または Client ID 対応が必要かは実装時に確認。→ 現時点では App ID（数値）で動作するため、MVP では App ID を使用して問題ない。

2. **Installation Token のキャッシュ戦略**: worker プロセスで複数の installation に対して並行処理する場合、`Octocrab` インスタンスの管理方法（installation ごとにインスタンスプール？毎回生成？）は実装設計時に決定。

3. **octocrab のエラー型と BoardFlow のエラー型のマッピング**: octocrab は `snafu` ベースのエラー型を使用。BoardFlow は `thiserror` を使用。変換レイヤーの設計が必要。

## 参照URL

- octocrab crate: https://crates.io/crates/octocrab
- octocrab docs.rs: https://docs.rs/octocrab/latest/octocrab/
- octocrab GitHub: https://github.com/XAMPPRocky/octocrab
- octocrab auth module: https://docs.rs/octocrab/latest/octocrab/auth/index.html
- octocrab IssueHandler: https://docs.rs/octocrab/latest/octocrab/issues/struct.IssueHandler.html
- octocrab create_jwt: https://docs.rs/octocrab/latest/octocrab/auth/fn.create_jwt.html
- jsonwebtoken crate: https://crates.io/crates/jsonwebtoken
- GitHub App JWT 生成: https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app
- GitHub Installation Token API: https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app
- GitHub App 認証概要: https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/about-authentication-with-a-github-app
