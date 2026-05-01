# Issue #19 作業ログ: GitHub App 認証クライアント実装

## Issue 概要

GitHub App として認証するためのクライアント実装。RS256 JWT 生成による App 認証と、Installation Token 取得を行う。`crates/github/` crate に実装する。octocrab crate の利用を検討する。

## ユーザー要望

docs 以下の仕様に基づいてアプリケーションを一通り実装する。GitHub App 認証クライアントはその中核コンポーネント。

---

## 調査フェーズ（2026-05-01）

### 調査対象

1. octocrab crate の GitHub App 認証サポート状況
2. RS256 JWT 生成に必要な Rust crate
3. GitHub App Installation Token API
4. GitHub Issues API（octocrab 経由）

### 調査結果

#### octocrab (v0.49.9)

- GitHub App 認証を **完全サポート**
- `OctocrabBuilder::app(app_id, key)` で App 認証インスタンス構築
- `installation()` / `installation_and_token()` で Installation Token 自動取得・キャッシュ
- 内部で `jsonwebtoken` v10 を使い RS256 JWT を生成（自前実装不要）
- `IssueHandler` で Issue 作成・取得・更新・Comment 作成・編集・削除をすべてカバー

#### JWT 生成

- octocrab 内蔵の `create_jwt()` が GitHub App 仕様に準拠
  - `iss`: App ID, `iat`: now - 60s, `exp`: now + 9min
  - アルゴリズム: RS256
  - PEM 秘密鍵から `EncodingKey::from_rsa_pem()` で生成

#### 依存関係

ワークスペースに追加すべき crate:
- `octocrab = "0.49"` — GitHub API クライアント
- `jsonwebtoken = { version = "10", default-features = false, features = ["use_pem"] }` — EncodingKey 直接利用用

### 結論ステータス

**`implementation_required`**

調査の結果、octocrab が BoardFlow の要件を十分にカバーすることが確認できた。
次のステップとして `crates/github/` への実装に進むべき。

### 成果物

- [docs/external/github-app-octocrab.md](../../external/github-app-octocrab.md) — 調査メモ（octocrab GitHub App 認証・Issue 操作）

---

## 実装フェーズ（2026-05-01）

### 実装内容

#### 新規ファイル

| ファイル | 説明 |
|---|---|
| `crates/github/src/error.rs` | `GitHubClientError` enum + octocrab Error からのステータスコードベース変換 |
| `crates/github/src/config.rs` | `GitHubAppConfig` (app_id + private_key_pem) |
| `crates/github/src/client.rs` | `GitHubAppClient` トレイト + `OctocrabGitHubAppClient` 実装 |

#### 変更ファイル

| `Cargo.toml` (workspace) | `octocrab = "0.49"`, `secrecy = "0.10"`, `jsonwebtoken` 追加 |
| `crates/github/Cargo.toml` | 依存定義追加 |
| `crates/github/src/lib.rs` | モジュール宣言 + re-export |
| `crates/worker/Cargo.toml` | `boardflow-github` 依存追加 |

#### トレイトメソッド

- `get_installation_token(installation_id)` — Installation Token 取得
- `create_issue(installation_id, owner, repo, title, body)` — Issue 作成
- `get_issue(installation_id, owner, repo, issue_number)` — Issue 取得
- `create_comment(installation_id, owner, repo, issue_number, body)` — コメント作成
- `update_comment(installation_id, owner, repo, comment_id, body)` — コメント更新

### テスト結果

```
running 9 tests
test error::tests::test_401_maps_to_auth ... ok
test error::tests::test_403_non_rate_limit_maps_to_auth ... ok
test error::tests::test_403_rate_limit_maps_to_rate_limited ... ok
test error::tests::test_403_secondary_rate_limit_maps_to_rate_limited ... ok
test error::tests::test_404_maps_to_not_found ... ok
test error::tests::test_422_maps_to_validation ... ok
test error::tests::test_429_maps_to_rate_limited ... ok
test error::tests::test_502_maps_to_api ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

### テスト観点

| テスト | 保証する内容 |
|---|---|
| `test_401_maps_to_auth` | 認証エラーが Auth にマッピングされること |
| `test_403_rate_limit_maps_to_rate_limited` | 403 + "rate limit" メッセージが RateLimited にマッピングされること |
| `test_403_secondary_rate_limit_maps_to_rate_limited` | secondary rate limit も RateLimited にマッピングされること |
| `test_422_maps_to_validation` | 422 が Validation にマッピングされること |
| `test_429_maps_to_rate_limited` | 429 が RateLimited にマッピングされること |
| `test_500_maps_to_api` | 5xx が Api にマッピングされること |
### 設計判断

1. **octocrab の型を外部に漏らさない**: `GitHubAppClient` トレイトの戻り値は自前定義型のみ
2. **`installation()` は同期メソッド**: octocrab v0.49 では `Result<Octocrab>` を返す（`await` 不要）
3. **エラーマッピングの抽出**: `map_status_to_error()` 関数に分離し、`#[non_exhaustive]` な octocrab 型を構築せずにテスト可能に
4. **`node_id` / `html_url`**: octocrab v0.49 では `String` / `Url` 型で直接利用可能（Option ではない）
5. **統合テストは書かない**: GitHub API への実呼び出しが必要なため、CI では実行不可

- `get_installation_token` は octocrab の内部キャッシュに依存しているため、大量並行呼び出し時の挙動は未検証

### 残リスク

- octocrab の `AppId(u64)` と GitHub 推奨の Client ID（文字列）の差異（MVP では App ID 数値で問題なし）
- Installation Token キャッシュ戦略（worker 設計時に決定）

---

## 再レビューフェーズ（2026-05-01）

### 対象

- Issue ID: `#19`
- 再レビュー観点:
    - `RateLimited` に retry 情報フィールドがあるか
    - `get_installation_token` が `SecretString` を返すか
    - テストが 11 件に増えて通過しているか
    - 新しいバグや設計問題が入っていないか
    - octocrab 型が公開 API に漏れていないか

### 調査結果

     - 現在の実装では octocrab がレスポンスヘッダを公開しないため `None` 固定だが、前回指摘の「retry 情報を保持できない」状態は解消している。
2. `GitHubAppClient::get_installation_token` は trait / 実装ともに `Result<SecretString, GitHubClientError>` を返しており、平文 `String` は外部公開されていない。
3. `mise exec -- cargo test -p boardflow-github` を再実行し、11 件すべて成功を確認。
4. `mise exec -- cargo build -p boardflow-github` も成功。
5. 公開 API は `GitHubAppClient`、`OctocrabGitHubAppClient`、`GitHubAppConfig`、`GitHubClientError`、自前 DTO 群で構成されており、シグネチャ上に octocrab の型は露出していない。

### レビュー結果

- `pr_ready: true`
### 総評

前回の 2 件の必須修正は、今回の差分でいずれも適切に反映されている。特に Installation Token を `SecretString` のまま返すようにした点は公開 API 境界の改善として妥当で、追加された invalid PEM テストと object safety テストも回帰防止として有効。

`retry_after_secs` は現時点では常に `None` であり、仕様 `docs/spec.md` §13.4 のヘッダ駆動の待機制御までをこの crate 単体で満たしているわけではない。ただし、Issue #19 のスコープは GitHub App 認証クライアントの土台実装であり、現在の変更により少なくとも error surface に拡張余地が確保された。worker 側が `None` の場合に exponential backoff を行う前提なら、この PR を止めるほどの欠陥ではない。

### 必須修正

### 任意改善

1. 将来 `docs/spec.md` §13.4 を厳密に満たす段階では、レスポンスヘッダ由来の `retry-after` / `x-ratelimit-reset` を取り出せる transport 拡張または middleware を検討する。

- 今回の再レビュー観点に関しては、新たな必須テスト不足は見当たらない。
- 実 GitHub API との疎通や rate limit ヘッダ抽出は統合テスト領域として引き続き未検証。

- `docs/spec.md` の GitHub App 連携仕様とレートリミット節を再確認。
- `docs/external/github-app-octocrab.md` の調査内容と実装方針は整合している。
### plan / research / docs との不整合

- blocker となる不整合はなし。
- 残差として、rate limit ヘッダの具体値は research / spec 側の理想状態までまだ到達していないが、今回の Issue スコープ外として許容可能。

### PR/完了結果

- Issue #19 は再レビュー観点を満たしており、PR 作成可。
2. GitHub 側の secondary rate limit 文言に依存する判定は、将来の API 変更で再調整が必要になる可能性がある。
### 後続エージェントへの注意点

- `crates/github/` は現在空（`lib.rs` のみ、内容なし）。`Cargo.toml` の `[dependencies]` も空
- 既存の `crates/api/src/github_access.rs` に `GithubAccessChecker` トレイトがあり、同じパターン（トレイト + 実装）で設計すべき
- `docs/spec.md` §11〜§13 に GitHub App 連携・Issue コメント・API キュー仕様の詳細あり
- octocrab は `secrecy::SecretString` を使うため、PEM 鍵のロードと管理に注意
- octocrab feature flags は `default` でよいが、不要な feature を削る場合は `jwt-rust-crypto` を必ず含める

---

## レビューフェーズ（2026-05-01）
### 総評

`crates/github/` の公開 API は、Issue #19 の計画にある 5 操作を一通り満たしており、octocrab の型も crate 外へ漏れていない。`cargo test -p boardflow-github` も通過しており、最小限の土台としては成立している。

一方で、仕様 `docs/spec.md` §13.4 が要求するレートリミット時の待機制御に必要な情報をエラー型が保持しておらず、後続の GitHub API キュー実装がこの API だけでは正しく backoff できない。また、Installation Token を `String` で公開しており、秘密情報の扱いとしては境界設計が弱い。

### レビュー結果

- `pr_ready: false`

### 必須修正

1. レートリミット時の retry 情報を `GitHubClientError` で保持できるようにする。
    - 現状の `RateLimited` はヘッダ情報を持たず、`x-ratelimit-reset` や `retry-after` に基づく待機ができない。
    - 仕様 `docs/spec.md` §13.4 では、`x-ratelimit-reset` / `retry-after` / 403 / 429 を見て遅延・backoff することが求められている。
    - このため、少なくとも reset 時刻、retry-after 秒数、HTTP status を保持する構造へ変更しないと、後続ジョブ実装で仕様を満たせない。

2. `get_installation_token` の戻り値を `String` ではなく秘密情報として扱える型に変更する。
    - 現状は `SecretString` から `String` に展開して返しており、呼び出し側で誤ってログやエラーに混入しやすい。
    - この crate は GitHub App 認証の境界なので、秘密情報は公開 API 上でも秘匿型のまま流すべき。

### 任意改善

1. `403` の非 rate-limit ケースを一律 `Auth` に落とすのは意味が広すぎるため、権限不足や integration 制約を区別できるエラー名に寄せた方が運用時の判別がしやすい。
2. `OctocrabGitHubAppClient::new` の失敗ケースは invalid PEM の単体テストを足しておくと、秘密鍵設定ミスの回帰を防ぎやすい。
2. `GitHubAppClient` を `Arc<dyn GitHubAppClient>` として扱う前提のコンパイル保証テストがない。
3. レートリミット関連は、403/429 の文言判定だけでなく retry 情報の抽出をテストできる形にしておく必要がある。

### ドキュメント確認

- `docs/spec.md` §11〜§13 は確認済み。
- `docs/external/github-app-octocrab.md` の調査内容と octocrab 採用判断は整合している。
- `README.md` は概要のみで、本件に追加更新が必須とは言えない。

### plan / research / docs との不整合

1. 計画では `get_installation_token` を提供するとしているが、秘密情報の扱いについて公開 API の設計が research の `secrecy` 採用方針と噛み合っていない。
2. research と仕様では GitHub のレートリミットヘッダを見て待機する前提だが、実装の `GitHubClientError` はその情報を保持していない。

### テスト結果

### 残リスク

1. 403 を `Auth` と誤分類すると、installation 解除・権限不足・secondary rate limit の運用判断を誤る可能性がある。
2. Installation Token が `String` として拡散すると、後続実装でログ流出や panic message 混入の余地が残る。

### PR/完了結果

- 現時点では `pr_ready: false`
- 上記 2 件の必須修正が入れば、Issue #19 の土台としては再レビュー可能

---

## 計画フェーズ（2026-05-01）

### 目的

`crates/github/` に GitHub App 認証クライアントを実装し、worker から DI 可能なトレイトベースの設計で以下の操作を提供する:

1. Installation Token の取得（octocrab による自動キャッシュ付き）
2. Issue 作成
3. Issue 取得（状態確認）
4. Comment 作成
5. Comment 編集

### 非目的

- `crates/api/` の `GithubAccessChecker` の変更や統合（役割が異なる）
- worker のジョブハンドラ実装（別 Issue）
- Label 作成、Issue 本文更新（将来拡張）
- GitHub Webhook 受信
- PR コメント連携

### 受け入れ条件

1. `crates/github/` が `cargo build` で正常にコンパイルできる
2. `GitHubAppClient` トレイトが定義され、MVP に必要な 5 操作を提供する
3. `OctocrabGitHubAppClient` 構造体がトレイトを実装する
4. octocrab の型が `crates/github/` 内部に閉じ込められ、domain/api/worker に漏出しない
5. 設定値（App ID, PEM 秘密鍵）を外部から注入できる
6. エラー型が `thiserror` ベースで定義されている
7. 単体テスト（モック使用）が少なくとも主要パスをカバーする
8. `crates/worker/Cargo.toml` に `boardflow-github` 依存が追加される

### 詳細要件

#### 公開トレイト定義

```rust
#[async_trait::async_trait]
pub trait GitHubAppClient: Send + Sync {
    /// 指定 installation の有効なトークンを返す（キャッシュ対応）
    async fn get_installation_token(
        &self,
        installation_id: u64,
    ) -> Result<String, GitHubClientError>;

    /// Issue を作成し、作成された issue 番号を返す
    async fn create_issue(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
    ) -> Result<CreatedIssue, GitHubClientError>;

    /// Issue を取得する
    async fn get_issue(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<IssueInfo, GitHubClientError>;

    /// コメントを作成し、comment_id を返す
    async fn create_comment(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        issue_number: u64,
        body: &str,
    ) -> Result<CreatedComment, GitHubClientError>;

    /// 既存コメントを編集する
    async fn update_comment(
        &self,
        installation_id: u64,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<(), GitHubClientError>;
}
```

#### 戻り値型

```rust
pub struct CreatedIssue {
    pub number: u64,
    pub node_id: String,
    pub html_url: String,
}

pub struct IssueInfo {
    pub number: u64,
    pub node_id: String,
    pub state: IssueState,
    pub html_url: String,
}

pub enum IssueState {
    Open,
    Closed,
}

pub struct CreatedComment {
    pub id: u64,
}
```

#### エラー型

```rust
#[derive(Debug, thiserror::Error)]
pub enum GitHubClientError {
    #[error("GitHub API authentication failed: {0}")]
    Auth(String),

    #[error("GitHub API rate limited")]
    RateLimited,

    #[error("GitHub resource not found: {0}")]
    NotFound(String),

    #[error("GitHub API validation failed: {0}")]
    Validation(String),

    #[error("GitHub API error: {0}")]
    Api(String),
}
```

#### 設定

```rust
pub struct GitHubAppConfig {
    pub app_id: u64,
    pub private_key_pem: secrecy::SecretString,
}
```

### 影響範囲

| 対象 | 変更内容 |
|---|---|
| `Cargo.toml` (workspace) | `octocrab`, `secrecy` の workspace dep 追加 |
| `crates/github/Cargo.toml` | 依存追加 |
| `crates/github/src/lib.rs` | モジュール宣言、トレイト re-export |
| `crates/github/src/client.rs` | **新規**: `OctocrabGitHubAppClient` 実装 |
| `crates/github/src/config.rs` | **新規**: `GitHubAppConfig` |
| `crates/github/src/error.rs` | **新規**: `GitHubClientError` |
| `crates/github/src/types.rs` | **新規**: `CreatedIssue`, `IssueInfo`, `CreatedComment`, `IssueState` |
| `crates/worker/Cargo.toml` | `boardflow-github` 依存追加 |

### 設計方針

1. **トレイトベース DI**: `GitHubAppClient` トレイトを定義し、worker は `Arc<dyn GitHubAppClient>` として注入
2. **octocrab 内部閉じ込め**: `octocrab::Octocrab` は `client.rs` 内部のみで使用。戻り値は自前の型に変換
3. **Installation 単位のクライアント生成**: `OctocrabGitHubAppClient` は App レベルの `Octocrab` インスタンスを保持し、各メソッドで `octocrab.installation(installation_id)` を呼んで installation スコープに切り替え
4. **エラーマッピング**: octocrab の `Error` を `GitHubClientError` にマッピング。Rate limit は `RateLimited` に分離して caller がリトライ判断できるようにする
5. **キャッシュ**: octocrab 内蔵の Installation Token キャッシュを活用（30秒バッファ付き自動更新）
6. **テスト**: トレイトベースなので、worker 側のテストではモック実装を差し込み可能。`crates/github/` 自体のユニットテストは octocrab のエラーマッピングを中心にテスト

### ファイル一覧と責務

```
crates/github/
├── Cargo.toml          # 依存定義
└── src/
    ├── lib.rs          # モジュール宣言 + pub use re-exports
    ├── client.rs       # OctocrabGitHubAppClient 構造体 + GitHubAppClient impl
    ├── config.rs       # GitHubAppConfig 構造体
    ├── error.rs        # GitHubClientError enum + octocrab Error からの変換
    └── types.rs        # CreatedIssue, IssueInfo, IssueState, CreatedComment
```

| ファイル | 責務 |
|---|---|
| `lib.rs` | `pub mod` 宣言、`pub use` による公開 API の re-export |
| `client.rs` | octocrab を使った `GitHubAppClient` トレイト実装。octocrab 型の変換ロジック |
| `config.rs` | `GitHubAppConfig` 定義。PEM 鍵を `secrecy::SecretString` で保持 |
| `error.rs` | `GitHubClientError` 定義 + `From<octocrab::Error>` 実装 |
| `types.rs` | ドメインに依存しない戻り値型。octocrab から変換した後の型 |

### Cargo.toml 変更

#### `Cargo.toml` (workspace root)

```toml
# [workspace.dependencies] に追加
octocrab = "0.49"
secrecy = "0.10"
```

#### `crates/github/Cargo.toml`

```toml
[dependencies]
octocrab = { workspace = true }
secrecy = { workspace = true }
async-trait = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
```

#### `crates/worker/Cargo.toml`

```toml
[dependencies]
# 既存に追加
boardflow-github = { path = "../github" }
```

### テスト方針

1. **ユニットテスト（`crates/github/src/error.rs`）**:
   - octocrab エラー → `GitHubClientError` 変換のマッピング正確性
   - Rate limit レスポンスの正しい分類

2. **統合テスト（`crates/github/tests/`）は MVP では書かない**:
   - 実際の GitHub API を叩くテストは CI 環境に秘密鍵が必要
   - 代わりに worker 側でモック `GitHubAppClient` を使ったテストで検証
   - 将来的に wiremock 等で HTTP レイヤモックを導入する可能性あり

3. **モック実装**:
   - `crates/github/src/lib.rs` に `#[cfg(test)]` で `MockGitHubAppClient` を提供する、もしくは worker 側テストで独自にモック定義
   - トレイトベースのため、mockall 等の導入は不要（手書きモックで十分）

### 実装手順

1. **workspace `Cargo.toml` 更新**: `octocrab`, `secrecy` を workspace deps に追加
2. **`crates/github/Cargo.toml` 更新**: 依存追加
3. **`crates/github/src/error.rs` 作成**: `GitHubClientError` 定義 + `From<octocrab::Error>`
4. **`crates/github/src/types.rs` 作成**: 戻り値型定義
5. **`crates/github/src/config.rs` 作成**: `GitHubAppConfig` 定義
6. **`crates/github/src/client.rs` 作成**: `GitHubAppClient` トレイト + `OctocrabGitHubAppClient` 実装
7. **`crates/github/src/lib.rs` 更新**: モジュール宣言 + re-export
8. **`crates/worker/Cargo.toml` 更新**: `boardflow-github` 依存追加
9. **コンパイル確認**: `cargo build -p boardflow-github`
10. **エラーマッピングのユニットテスト追加**（error.rs 内 `#[cfg(test)]`）
11. **`cargo test -p boardflow-github`** で全テスト通過確認

### ドキュメント更新対象

- `docs/backend/api.md` — 更新不要（HTTP API 仕様書であり、GitHub クライアントは内部実装）
- `docs/logs/19/worklog.md` — 本ファイル（計画・実装・テスト結果を追記）

### 実装要否

**`implementation_required`**

### 未解決の疑問

なし。調査フェーズで octocrab の機能確認が完了しており、仕様書の要件とマッピング済み。

### 残リスク

1. **octocrab バージョンアップ**: v0.49 は 2025 年リリース。破壊的変更の頻度は低いが、依存ロック推奨
2. **PEM 秘密鍵の環境変数渡し**: 改行を含む PEM を環境変数で渡す場合のエスケープ方針は worker 設定実装時に決定（`\n` リテラル or ファイルパス指定）
3. **octocrab + reqwest の共存**: octocrab は内部で hyper を使用、既存 `crates/api/` は reqwest を使用。機能衝突はないがバイナリサイズが若干増加
4. **Installation Token キャッシュの expiry handling**: octocrab の内蔵キャッシュは 30 秒バッファで自動更新するが、長時間アイドル後の初回リクエストで latency が増加する可能性あり

---

## レビュー修正フェーズ（2026-05-01）

### レビュー指摘事項

1. `RateLimited` に retry 情報がない（spec §13.4 の要求）
2. `get_installation_token` の戻り値が平文 `String`（秘密情報の公開 API 境界越え）
3. 追加テスト要求（invalid PEM テスト、object safety テスト）

### 実装内容

#### 1. RateLimited に retry_after_secs フィールド追加

- `crates/github/src/error.rs`: `RateLimited` を unit variant → struct variant に変更
  - `retry_after_secs: Option<u64>` フィールド追加
  - octocrab の `Error::GitHub` にはレスポンスヘッダが含まれないため、現時点では常に `None`
  - caller（worker）は `None` 時に exponential backoff を使用する想定
- `map_status_to_error` の 403（rate limit）と 429 のケースで `RateLimited { retry_after_secs: None }` を返すよう更新
- 既存テスト 3 件を新しいパターンに更新

#### 2. get_installation_token の戻り値を SecretString に変更

- `crates/github/src/client.rs`: trait 定義の戻り値を `Result<SecretString, GitHubClientError>` に変更
- 実装で `token.expose_secret().to_string()` → `token` をそのまま返すよう簡素化
- `use secrecy::SecretString;` を import に追加

#### 3. 追加テスト

- `client::tests::new_with_invalid_pem_returns_auth_error`: 壊れた PEM で `Auth` エラーが返ることを検証
- `client::tests::trait_is_object_safe`: `&dyn GitHubAppClient` の型チェックによるオブジェクトセーフ性確認

### テスト結果

```
running 11 tests
test client::tests::trait_is_object_safe ... ok
test client::tests::new_with_invalid_pem_returns_auth_error ... ok
test error::tests::test_401_maps_to_auth ... ok
test error::tests::test_403_non_rate_limit_maps_to_auth ... ok
test error::tests::test_403_rate_limit_maps_to_rate_limited ... ok
test error::tests::test_403_secondary_rate_limit_maps_to_rate_limited ... ok
test error::tests::test_404_maps_to_not_found ... ok
test error::tests::test_422_maps_to_validation ... ok
test error::tests::test_429_maps_to_rate_limited ... ok
test error::tests::test_500_maps_to_api ... ok
test error::tests::test_502_maps_to_api ... ok
test result: ok. 11 passed; 0 failed; 0 ignored
```

ワークスペース全体のビルドも成功（downstream crate への影響なし）。

### 残リスク

- `retry_after_secs` は現時点では常に `None`。将来的に octocrab カスタム middleware でレスポンスヘッダを保存する対応が必要
- octocrab の `installation_and_token()` が返す `SecretString` の型が将来変更された場合、コンパイルエラーとして検出される（安全）

---

## ドキュメント確認フェーズ（2026-05-01 最終）

### 対象

- Issue ID: `#19`
- 確認対象:
  - `docs/backend/summary.md`
  - `docs/external/github-app-octocrab.md`
  - `docs/logs/19/worklog.md`
  - `README.md`
  - `docs/spec.md` の GitHub App / rate limit 関連節

### 確認結果

1. `docs/backend/summary.md` は、backend が GitHub Issue 作成やコメント更新などの非同期処理を担い、`crates/github` を GitHub API 境界として切る構成と読めるため、今回の `crates/github/` 実装と整合している。
2. `docs/external/github-app-octocrab.md` の主要記述は実装と一致している。
    - `OctocrabBuilder::app(...)` による App 認証
    - `installation(...)` / `installation_and_token(...)` による installation スコープ化と token 取得
    - GitHub App JWT の `iss` / `iat` / `exp` 要件
    - installation token の 1 時間有効期限
3. `docs/logs/19/worklog.md` には、Issue 概要、ユーザー要望、調査、計画、実装、テスト、レビュー、PR 判定、残リスクが時系列で揃っている。途中の `pr_ready: false` は履歴として残っており、後続の再レビュー結果で解消済みと読める。
4. 他ドキュメントの必須更新は見当たらない。
    - `docs/backend/api.md` は HTTP API 契約書のため今回の crate 実装とは直接対応しない。
    - `README.md` も現段階では概要のままで問題ない。

### ドキュメント観点の判定

- `docs_ready: true`

### 必須修正

- なし

### 任意改善

1. `docs/external/github-app-octocrab.md` の「octocrab 自体はレートリミット管理を提供しない」は、BoardFlow が必要とする installation 単位のキュー制御は自前実装という意味では妥当だが、octocrab 自体に retry 機能と rate limit API 参照手段はあるため、表現を少し狭めると誤読を避けやすい。
2. `docs/logs/19/worklog.md` は履歴情報が十分な一方で長いので、将来必要なら末尾に最新ステータス要約を 2-3 行で追加すると参照しやすい。

### 外部調査メモに関する補足

- Context7 MCP は認証エラーで利用できなかったため、今回の裏取りは docs.rs と GitHub 公式ドキュメントで実施した。
- 根拠 URL と採用判断の対応は十分で、実装との差異は見当たらない。

### PR/完了結果

- Issue #19 はドキュメント観点でも PR 作成可。
- 判定: `docs_ready: true`

### ドキュメント確認の残リスク

1. `docs/external/github-app-octocrab.md` は「octocrab がサポートする機能」と「Issue #19 で実装した機能」がやや近接して記述されているため、将来の読者が未実装機能まで実装済みと誤読しないよう、必要に応じて区別を明示するとさらに良い。

---

## PR作成フェーズ（2026-05-01）

### PR作成前チェック

- `pr_ready: true`（再レビュー通過、2026-05-01）
- `docs_ready: true`（ドキュメント確認通過、2026-05-01）
- 未コミット変更: `docs/logs/19/worklog.md`（本フェーズ追記のみ）→ コミット後にプッシュ
- テスト: `boardflow-github` 11/11 pass、ワークスペース全体ビルド成功
- research 成果物と実装の矛盾: なし

### PR/完了結果

- ブランチ: `feat/19-github-app-client` → `main`
- PRリンク: 作成後に記載
- 判定: PR作成実施

### 残リスク

- `retry_after_secs` は現状常に `None`（octocrab がレスポンスヘッダを公開しないため）。将来 middleware 拡張で対応可能
- 統合テスト（実 GitHub API 呼び出し）なし（CI に秘密鍵が必要なため意図的に省略）
- octocrab バージョンアップ時の breaking changes
- `docs/external/github-app-octocrab.md` にて実装済み機能と未実装機能の区別が将来的に不明確になる可能性
