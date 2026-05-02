# Issue #28 作業ログ: GitHub App Webhook受信エンドポイント実装

## Issue概要

GitHub App からの Webhook を受信するエンドポイントを実装する。
installation 関連のイベント（installation created/deleted、repositories added/removed）を処理し、repository 情報を DB 同期する。webhook 署名の検証も行う。

## Issueまでの経緯

- Issue #19（GitHub App クライアント）がマージ済み。`crates/github/` に octocrab ベースのクライアントが実装されている。
- `docs/spec.md` および `docs/backend/summary.md` で GitHub App webhook は MVP から含めると明記。
- `docs/backend/summary.md` のテスト方針に「GitHub webhook signature verification test」が含まれている。
- api crate には既に `hmac`, `sha2`, `hex` が依存に含まれている。

## ユーザー要望

docs 以下の仕様に基づいてアプリケーションを一通り実装する。

## 調査結果

### 2026-05-02: 外部調査完了 (research agent)

以下の外部トピックを調査し、`docs/external/github-webhook-signature.md` に記録した。

#### 1. GitHub App Webhook 署名検証
- HMAC-SHA256 アルゴリズムで `X-Hub-Signature-256` ヘッダを検証
- Rust では `hmac` + `sha2` + `hex` crate（全て api crate に既存）で実装可能
- `hmac` crate の `verify_slice()` で定数時間比較が組み込み済み
- 追加 crate 不要

#### 2. Webhook Payload 構造
- `installation` イベント: action = created / deleted
  - `installation.id`, `repositories[].id`, `repositories[].full_name` 等を含む
- `installation_repositories` イベント: action = added / removed
  - `repositories_added[]`, `repositories_removed[]` で差分を受信
- `ping` イベント: webhook 設定確認用、200 OK 返却のみ

#### 3. ベストプラクティス
- 10秒以内に応答する
- `X-GitHub-Delivery` で冪等性を確保可能（MVPでは upsert で十分）
- raw body に対して署名検証を行う（JSON パース後のデータではなく）

#### 4. Axum での実装パターン
- `HeaderMap` + `Bytes` を handler 引数に並べる
- `Bytes` は body を消費するため最後に配置
- 署名検証後に同じ `Bytes` から `serde_json::from_slice` でパース

### 参照URL
- https://docs.github.com/en/webhooks/using-webhooks/securing-your-webhooks
- https://docs.github.com/en/webhooks/webhook-events-and-payloads#installation
- https://docs.github.com/en/webhooks/webhook-events-and-payloads#installation_repositories
- https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks

## 結論ステータス

**`implementation_required`**

調査は完了し、実装に必要な情報はすべて揃っている。以下が後続実装で必要:

1. `AppConfig` に `github_webhook_secret` を追加
2. `POST /api/v1/github/webhook` エンドポイントを api crate に追加
3. 署名検証関数の実装（`hmac` + `sha2` + `hex`）
4. Webhook payload の serde 型定義
5. installation/repository の DB 同期ロジック（upsert）
6. 署名検証のユニットテスト
7. webhook handler の統合テスト

## 残リスク

- なし（MVPに必要な外部情報は十分）

---

## 計画フェーズ (2026-05-02)

### 実装要否: `implementation_required`

### 目的

GitHub App からの Webhook を受信・検証し、installation/repository の状態を DB に同期するエンドポイントを実装する。これにより、GitHub App がインストール/アンインストールされたときやリポジトリが追加/削除されたときに、BoardFlow の repositories テーブルが自動的に更新される。

### 非目的

- push イベントの処理（Webhookではなく Action API 経由で処理）
- Webhook delivery ID による冪等性チェック（upsert で十分）
- Redis / Queue への enqueue（同期処理で十分な軽量 upsert のみ）
- installation テーブルの作成（MVP では repositories テーブルの installation_id で管理）
- OpenAPI ドキュメント登録（webhook は外部からの呼び出しであり、utoipa ルート登録不要）

### 受け入れ条件

1. `POST /api/v1/github/webhook` でリクエストを受信できる
2. `X-Hub-Signature-256` ヘッダによる HMAC-SHA256 署名検証が行われる
3. 署名不正時は 401 を返す
4. `ping` イベントで 200 を返す
5. `installation` created で repositories テーブルに upsert される
6. `installation` deleted で該当 installation_id のリポジトリの installation_id が 0 にクリアされる
7. `installation_repositories` added で repositories テーブルに upsert される
8. `installation_repositories` removed で該当リポジトリの installation_id が 0 にクリアされる
9. 未対応イベントは 200 を返して無視する
10. `GITHUB_WEBHOOK_SECRET` 未設定時は webhook エンドポイントが 500 を返す（署名検証不可のため）
11. 署名検証のユニットテストがある
12. webhook handler の統合テスト（DB あり）がある

### 詳細要件

#### 1. AppConfig 変更

`AppConfig` に `github_webhook_secret: Option<String>` を追加。環境変数 `GITHUB_WEBHOOK_SECRET` から読み込む。

#### 2. 署名検証関数

```rust
// crates/api/src/routes/webhook.rs 内
fn verify_signature(secret: &[u8], body: &[u8], signature_header: &str) -> bool
```

- `sha256=` プレフィックスを除去
- `hex::decode` で期待値をバイト列に変換
- `Hmac<Sha256>` で HMAC を計算
- `mac.verify_slice()` で定数時間比較

#### 3. Webhook Payload serde 型

```rust
// crates/api/src/routes/webhook.rs 内（ファイルローカル）

#[derive(Deserialize)]
struct WebhookRepository {
    id: i64,
    name: String,
    full_name: String,
}

#[derive(Deserialize)]
struct WebhookInstallation {
    id: i64,
}

#[derive(Deserialize)]
struct InstallationEvent {
    action: String,
    installation: WebhookInstallation,
    #[serde(default)]
    repositories: Vec<WebhookRepository>,
}

#[derive(Deserialize)]
struct InstallationRepositoriesEvent {
    action: String,
    installation: WebhookInstallation,
    #[serde(default)]
    repositories_added: Vec<WebhookRepository>,
    #[serde(default)]
    repositories_removed: Vec<WebhookRepository>,
}
```

#### 4. Handler 関数

```rust
// crates/api/src/routes/webhook.rs

pub async fn github_webhook(
    State(pool): State<PgPool>,
    Extension(webhook_secret): Extension<WebhookSecret>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError>
```

処理フロー:
1. `webhook_secret.0` が None なら 500 Internal Error
2. `X-Hub-Signature-256` ヘッダを取得。なければ 401
3. `verify_signature()` で検証。失敗なら 401
4. `X-GitHub-Event` ヘッダでイベント種別を判定
5. イベントに応じた処理:
   - `ping` → 200 OK
   - `installation` → `handle_installation_event()`
   - `installation_repositories` → `handle_installation_repositories_event()`
   - その他 → 200 OK（ログ出力のみ）

#### 5. イベントハンドラ

**handle_installation_event:**
- `created`: `repositories` 配列の各リポジトリを `full_name` から owner/name を分割して upsert
- `deleted`: 該当 `installation_id` のリポジトリの `installation_id` を 0 にクリア

**handle_installation_repositories_event:**
- `added`: `repositories_added` の各リポジトリを upsert
- `removed`: `repositories_removed` の各リポジトリの `installation_id` を 0 にクリア

#### 6. DB クエリ追加

`crates/db/src/queries/repository.rs` に以下を追加:

```rust
pub async fn clear_installation(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    installation_id: i64,
) -> Result<Vec<Repository>, sqlx::Error>
// UPDATE repositories SET installation_id = 0, updated_at = NOW()
// WHERE installation_id = $1 RETURNING *

pub async fn clear_installation_for_repo(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    github_repository_id: i64,
) -> Result<Option<Repository>, sqlx::Error>
// UPDATE repositories SET installation_id = 0, updated_at = NOW()
// WHERE github_repository_id = $1 RETURNING *
```

#### 7. Extension 型

```rust
// crates/api/src/lib.rs に追加
#[derive(Clone)]
pub struct WebhookSecret(pub Option<String>);
```

#### 8. ルーター登録

`create_app_with_config` に:
- 引数 `webhook_secret: Option<String>` を追加
- `WebhookSecret` Extension を layer に追加
- `.route("/api/v1/github/webhook", post(routes::webhook::github_webhook))` を追加
  - OpenAPI ルーターではなく、通常の `.route()` で登録（utoipa 不要）

### 影響範囲

| ファイル | 変更内容 |
|---|---|
| `crates/api/src/config.rs` | `github_webhook_secret` フィールド追加 |
| `crates/api/src/lib.rs` | `WebhookSecret` 型追加、`create_app_with_config` 引数追加、route 登録、Extension layer 追加 |
| `crates/api/src/routes/mod.rs` | `pub mod webhook;` 追加 |
| `crates/api/src/routes/webhook.rs` | **新規作成** — handler、署名検証、payload 型 |
| `crates/api/src/main.rs` | `AppConfig` から `webhook_secret` を `create_app_with_config` に渡す |
| `crates/db/src/queries/repository.rs` | `clear_installation`、`clear_installation_for_repo` 追加 |
| `crates/api/tests/webhook_test.rs` | **新規作成** — 統合テスト |

### 設計方針

1. **署名検証は raw body に対して行う**: `Bytes` extractor で取得した生バイト列に対して HMAC 検証し、その後 `serde_json::from_slice` でパースする
2. **OpenAPI 登録しない**: Webhook は GitHub からの着信であり、BoardFlow の公開 API ドキュメントに載せる必要がない。通常の `Router::route()` で登録する
3. **同期処理**: DB upsert は軽量なので、queue 経由にせず handler 内で同期的に完了する
4. **installation 解除は論理削除**: `installation_id = 0` でクリアし、repository レコード自体は残す（BoardProject や BoardRun の参照整合性を保つ）
5. **create_app_with_config の引数追加**: 既存パターンに合わせて Option<String> で webhook_secret を渡す。テストではテスト用 secret を指定する

### 実装順序（TDD前提）

#### Step 1: 署名検証ユニットテスト & 実装
1. `crates/api/src/routes/webhook.rs` を作成
2. `verify_signature()` 関数を実装
3. GitHub 公式テスト値でユニットテスト（`#[cfg(test)]` モジュール内）:
   - secret: `"It's a Secret to Everybody"`, payload: `"Hello, World!"`
   - 正しい署名 → true
   - 不正な署名 → false
   - プレフィックスなし → false
   - 不正な hex → false

#### Step 2: Payload serde 型
1. `InstallationEvent`, `InstallationRepositoriesEvent` 等の型を定義
2. 調査結果の JSON サンプルでデシリアライズのユニットテスト

#### Step 3: DB クエリ追加
1. `clear_installation` を `crates/db/src/queries/repository.rs` に追加
2. `clear_installation_for_repo` を同ファイルに追加

#### Step 4: AppConfig & Extension 変更
1. `AppConfig.github_webhook_secret` 追加
2. `WebhookSecret` 型を `lib.rs` に追加
3. `create_app_with_config` の引数と layer 追加
4. `main.rs` で config から渡す

#### Step 5: Handler 実装
1. `github_webhook` handler を実装
2. `handle_installation_event`, `handle_installation_repositories_event` を実装
3. route 登録

#### Step 6: 統合テスト
1. `crates/api/tests/webhook_test.rs` を作成
2. テストケース:
   - **test_webhook_ping**: ping イベントで 200 が返る
   - **test_webhook_invalid_signature**: 不正な署名で 401 が返る
   - **test_webhook_missing_signature**: 署名ヘッダなしで 401 が返る
   - **test_webhook_installation_created**: installation created で repositories が upsert される
   - **test_webhook_installation_deleted**: installation deleted で installation_id が 0 にクリアされる
   - **test_webhook_repos_added**: installation_repositories added で upsert される
   - **test_webhook_repos_removed**: installation_repositories removed で installation_id が 0 にクリアされる
   - **test_webhook_unknown_event**: 未知イベントで 200 が返る

統合テストの構成:
- `#[serial]` 付き
- `setup_pool()` で DB 接続・マイグレーション
- `create_app_with_config` でテスト用 webhook_secret を設定
- HMAC-SHA256 でテスト用署名を計算
- `tower::ServiceExt::oneshot` でリクエスト送信
- DB 状態を `sqlx::query_as` で検証

### テスト観点

| テスト | 種別 | ファイル |
|---|---|---|
| verify_signature 正常系 | ユニット | `webhook.rs` 内 `#[cfg(test)]` |
| verify_signature 異常系（不正署名、prefix なし、不正 hex） | ユニット | 同上 |
| payload デシリアライズ | ユニット | 同上 |
| ping イベント | 統合 | `webhook_test.rs` |
| 署名検証失敗 | 統合 | 同上 |
| installation created → DB upsert | 統合 | 同上 |
| installation deleted → installation_id クリア | 統合 | 同上 |
| installation_repositories added → DB upsert | 統合 | 同上 |
| installation_repositories removed → installation_id クリア | 統合 | 同上 |
| 未知イベント → 200 | 統合 | 同上 |

### ドキュメント更新対象

- `docs/logs/28/worklog.md` — 本作業ログ（実装完了時にも追記）
- `docs/backend/api.md` への追記は不要（webhook は外部からの着信であり、BoardFlow の公開 API 仕様ではない）

### 未解決の疑問

なし。仕様・外部調査とも十分な情報がある。

### 更新した作業ログパス

`docs/logs/28/worklog.md`

---

## レビューフェーズ (2026-05-02)

### Issueまでの経緯

- 対象は Issue #28 のみ。
- 実装、research、spec、backend summary、統合テスト、関連 worker ハンドラを突き合わせてレビューした。

### 調査結果

- `docs/spec.md` / `docs/backend/summary.md` が求める GitHub App webhook 受信、署名検証、repository 同期の大枠は実装されている。
- 署名検証は raw body に対する HMAC-SHA256 + `verify_slice()` で、research と GitHub Docs の推奨に整合している。
- `installation_repositories.removed` の DB 更新が `github_repository_id` のみで実行されており、event payload の `installation.id` で絞っていない。
- 受け入れ条件 10 (`GITHUB_WEBHOOK_SECRET` 未設定時に 500) に対応する統合テストは存在しない。
- `GITHUB_WEBHOOK_SECRET` の運用設定が README / docs 配下の恒久ドキュメントに反映されていない。

### 実装内容レビュー結果

- `pr_ready: false`

#### 重大指摘

1. **major**: `installation_repositories removed` が installation を条件にせず repository 単位で解除しており、順不同 delivery や再配送で現行の連携状態を誤って消し得る。
    - 呼び出し側: `crates/api/src/routes/webhook.rs` で removal payload の `installation.id` を受け取っているにもかかわらず、[crates/api/src/routes/webhook.rs](crates/api/src/routes/webhook.rs#L210) から [crates/api/src/routes/webhook.rs](crates/api/src/routes/webhook.rs#L212) では `clear_installation_for_repo(pool, repo.id)` しか渡していない。
    - クエリ側: [crates/db/src/queries/repository.rs](crates/db/src/queries/repository.rs#L159) から [crates/db/src/queries/repository.rs](crates/db/src/queries/repository.rs#L165) は `WHERE github_repository_id = $1` だけで `installation_id` を見ていない。
    - GitHub webhook は再配送や順不同到着を考慮すべきなので、少なくとも `github_repository_id` と `installation_id` の両方で条件付ける必要がある。

#### 必須修正

1. `clear_installation_for_repo` を `github_repository_id + installation_id` 条件に変更し、handler 側から event の `installation.id` を渡す。
2. 上記に対応する統合テストを追加し、別 installation に再紐付け済みの repository に対して古い removal event が来ても `installation_id` を消さないことを検証する。
3. 受け入れ条件 10 に対応する統合テストを追加し、`WebhookSecret(None)` 構成で [crates/api/tests/webhook_test.rs](crates/api/tests/webhook_test.rs) に 500 応答を確認するケースを入れる。

#### 任意改善

1. `X-GitHub-Delivery` をログに含めると、再配送や障害解析時の追跡がしやすくなる。
2. 連携解除済み repository (`installation_id = 0`) を後続 worker が扱うと GitHub API 呼び出しで 0 を installation_id として使うため、将来的にはジョブ側で明示的に無効状態を扱うガードがあると安全。

### テスト結果

- 実行確認: `cargo test -p boardflow-api --test webhook_test` → 8件 pass
- 実行確認: `cargo test -p boardflow-api webhook::tests` → 11件 pass
- 既存の `create_app_with_config` 呼び出しは追加引数込みでコンパイル上は整合していることを確認した。

### テスト不足

- [crates/api/tests/webhook_test.rs](crates/api/tests/webhook_test.rs) には `GITHUB_WEBHOOK_SECRET` 未設定時 500 のケースがない。
- removal event の installation 不一致時に状態を保持する回帰テストがない。

### ドキュメント確認

- `docs/spec.md` / `docs/backend/summary.md` / `docs/external/github-webhook-signature.md` とは概ね整合。
- ただし `GITHUB_WEBHOOK_SECRET` の設定方法が README などの恒久ドキュメントに見当たらず、運用者向け導線が不足している。

### plan / research / docs との不整合

- 計画上の受け入れ条件 10 は実装されているが、テスト実績とテストファイル構成が追いついていない。
- research では delivery の信頼性と raw body 検証を重視しており、署名検証は一致している。一方で removal の条件が installation 非依存なのは、順不同 delivery を考慮する設計として弱い。

### PR/完了結果

- PR 作成可否: `pr_ready: false`
- 理由: 連携解除クエリの条件不足が実データ破壊に繋がり得るため。

### 残リスク

- installation deleted / removed 後に既存の GitHub job が残っていた場合の downstream 挙動は本 Issue のテスト範囲外。

### 更新した作業ログパス

`docs/logs/28/worklog.md`

---

## レビュー指摘修正フェーズ (2026-05-02)

### 修正内容

レビューで指摘された必須修正3件 + 任意改善1件をすべて対応した。

#### 1. (major) `clear_installation_for_repo` の installation 条件追加

- **`crates/db/src/queries/repository.rs`**: `clear_installation_for_repo` に `installation_id: i64` 引数を追加し、WHERE 条件を `github_repository_id = $1 AND installation_id = $2` に変更。再配送や installation 移動時に現行の installation を誤ってクリアしなくなった。
- **`crates/api/src/routes/webhook.rs`**: `handle_installation_repositories_event` の `removed` 処理で `event.installation.id` を第3引数として渡すように変更。

#### 2. (major) secret 未設定時 500 の統合テスト追加

- **`crates/api/tests/webhook_test.rs`**: `test_webhook_no_secret_configured` を追加。`webhook_secret = None` で app を構成し、500 が返ることを検証。

#### 3. (major) installation 不一致の removed event 回帰テスト追加

- **`crates/api/tests/webhook_test.rs`**: `test_webhook_repos_removed_different_installation` を追加。repo が installation_id=A で登録済みの状態で installation_id=B からの removed event が来ても installation_id が変わらないことを検証。

#### 4. (suggestion) X-GitHub-Delivery ログ出力

- **`crates/api/src/routes/webhook.rs`**: `github_webhook` handler で `X-GitHub-Delivery` ヘッダを取得し、`tracing::info!` に `delivery_id` フィールドとして含めるよう変更。

### テスト結果

- `cargo check --workspace`: OK
- `cargo test -p boardflow-api --lib`: 15 passed
- `cargo test -p boardflow-api --test webhook_test`: 10 passed (新規2件含む)
- `cargo clippy --workspace --all-targets -- -D warnings`: OK
- `cargo fmt --all -- --check`: OK

### 更新ドキュメント

- `docs/logs/28/worklog.md` (本ファイル)

### 残リスク

- installation deleted / removed 後に既存の GitHub job が残っていた場合の downstream 挙動は本 Issue のテスト範囲外。

### 更新した作業ログパス (修正後)

`docs/logs/28/worklog.md`

---

## 再レビューフェーズ (2026-05-02)

### Issueまでの経緯

- 対象は Issue #28 のみ。
- 前回レビューで `pr_ready: false` とした major 3件の修正有無を再確認した。
- ユーザー申告の修正点に加え、Issue本文、spec、research、現行実装、統合テストを突き合わせた。

### ユーザー要望

- 前回 major 3件が正しく解消されているか再レビューする。
- 特に以下を確認する。
    1. `clear_installation_for_repo` の SQL WHERE 句に `installation_id` が追加されていること
    2. handler 側で `event.installation.id` を渡していること
    3. 新規テスト2件がリグレッション防止として十分であること

### 調査結果

- Issue本文は `gh issue view 28 --json number,title,body` で確認した。
- [crates/db/src/queries/repository.rs](crates/db/src/queries/repository.rs#L159) の `clear_installation_for_repo` は `github_repository_id = $1 AND installation_id = $2` 条件に変更済み。
- [crates/api/src/routes/webhook.rs](crates/api/src/routes/webhook.rs#L220) の `removed` 処理は `clear_installation_for_repo(pool, repo.id, event.installation.id)` を呼ぶように変更済み。
- [crates/api/tests/webhook_test.rs](crates/api/tests/webhook_test.rs#L368) の `test_webhook_no_secret_configured` が `webhook_secret = None` 構成で 500 を検証している。
- [crates/api/tests/webhook_test.rs](crates/api/tests/webhook_test.rs#L398) の `test_webhook_repos_removed_different_installation` が installation 不一致時に `installation_id` を保持することを検証している。
- [crates/api/src/routes/webhook.rs](crates/api/src/routes/webhook.rs#L109) で `X-GitHub-Delivery` が `delivery_id` としてログ出力に含まれている。
- GitHub Docs のベストプラクティス上も、redelivery があり得ること、`X-GitHub-Delivery` で delivery を追跡すること、event/action を見て処理することは妥当。

### 計画

- major 3件の修正箇所を直接確認する。
- Webhook 統合テストを実行して新規2件を含む回帰防止を検証する。
- research / docs / spec と照合して、ブロッカーが残っていないか判定する。

### 実装内容

- `clear_installation_for_repo` は installation 条件付き更新に修正されており、前回指摘の誤解除リスクは解消された。
- handler 側は removal payload の `installation.id` を DB クエリに渡しており、SQL 修正と接続されている。
- 追加された2件の統合テストはいずれも前回の欠落点を直接カバーしている。

### テスト結果

- 実行確認: `cargo test -p boardflow-api --test webhook_test` → 10 passed
- 新規追加の `test_webhook_no_secret_configured` / `test_webhook_repos_removed_different_installation` を含めて成功した。

### レビュー結果

- `pr_ready: true`
- 前回の major 3件はすべて修正済みで、現時点で PR を止める指摘はない。
- 新規テスト2件は、受け入れ条件 10 と removal event の installation 不一致回帰の両方を直接押さえており、今回の修正範囲に対して十分。

### ドキュメント確認

- [docs/spec.md](docs/spec.md#L1606) と [docs/backend/summary.md](docs/backend/summary.md#L1) の GitHub App webhook 方針とは整合している。
- `GITHUB_WEBHOOK_SECRET` の恒久ドキュメント導線は引き続き薄いが、前回 major の再レビュー観点としては非ブロッカーと判断した。

### PR/完了結果

- PR 作成可否: `pr_ready: true`

### 残リスク

- Issue本文は `POST /api/v1/webhooks/github` と書かれている一方、実装とテストは `POST /api/v1/github/webhook` に統一されている。現行 docs / research / テストとは整合しているため今回はブロッカーにしないが、外部利用者向けの案内は将来的に一本化した方がよい。

### 更新した作業ログパス

`docs/logs/28/worklog.md`

---

## ドキュメント確認フェーズ (2026-05-02)

### Issueまでの経緯

- 対象は Issue #28 のみ。
- docs review として Issue本文、research成果物、実装概要、関連ドキュメント、現行実装、既存 worklog を突き合わせた。

### ユーザー要望

- `docs/spec.md`、`docs/backend/summary.md`、`docs/external/github-webhook-signature.md`、`docs/logs/28/worklog.md` の整合性を確認する。
- `GITHUB_WEBHOOK_SECRET` の運用ドキュメント追記要否を判断する。

### 調査結果

- [docs/spec.md](docs/spec.md#L1606) は GitHub App 連携方針と webhook 受信の存在を定義しており、今回の実装内容と矛盾しない。
- [docs/backend/summary.md](docs/backend/summary.md#L290) の「GitHub webhook signature verification test」は、[crates/api/src/routes/webhook.rs](crates/api/src/routes/webhook.rs#L217) のユニットテスト群と [crates/api/tests/webhook_test.rs](crates/api/tests/webhook_test.rs#L1) の統合テスト群でカバーされている。
- [docs/external/github-webhook-signature.md](docs/external/github-webhook-signature.md#L297) の実装方針は、[crates/api/src/routes/webhook.rs](crates/api/src/routes/webhook.rs#L56)、[crates/api/src/lib.rs](crates/api/src/lib.rs#L92)、[crates/db/src/queries/repository.rs](crates/db/src/queries/repository.rs#L151) と一致している。
- [docs/logs/28/worklog.md](docs/logs/28/worklog.md#L1) には経緯、調査、計画、実装、テスト、レビュー結果、再レビュー結果が揃っており、今回の確認結果を追記すれば時系列の完全性も満たせる。
- `GITHUB_WEBHOOK_SECRET` は [crates/api/src/config.rs](crates/api/src/config.rs#L39) と [crates/api/src/main.rs](crates/api/src/main.rs#L41) で実装に反映されているが、[README.md](README.md) と `docs/` 配下の恒久ドキュメントには設定手順が見当たらない。
- Issue本文は `POST /api/v1/webhooks/github`、実装と research は `POST /api/v1/github/webhook` であり、Issue本文とのみ表記揺れが残っている。

### 計画

- ブロッカーになる不整合があるかを docs 観点で判定する。
- 必須修正と任意改善を分離して worklog に記録する。

### 実装内容

- コード変更は行わず、docs review 結果のみ整理した。

### テスト結果

- 既存の実装ログに記載された webhook テスト結果を確認し、docs/backend/summary.md のテスト方針に対する裏付けとして妥当と判断した。

### レビュー結果

- `docs_ready: true`
- PR を docs 観点で止める必須修正はない。

### ドキュメント確認

- 整合しているドキュメント:
    - [docs/spec.md](docs/spec.md#L1606)
    - [docs/backend/summary.md](docs/backend/summary.md#L290)
    - [docs/external/github-webhook-signature.md](docs/external/github-webhook-signature.md#L346)
    - [docs/logs/28/worklog.md](docs/logs/28/worklog.md#L1)
- 不整合のあるドキュメント:
    - GitHub Issue #28 本文のエンドポイント表記が `POST /api/v1/webhooks/github` のままで、実装・research と一致していない。
- 不足しているドキュメント:
    - `GITHUB_WEBHOOK_SECRET` をどこで設定するかの運用メモが [README.md](README.md) または関連 docs にない。

### PR/完了結果

- docs 観点の PR 作成可否: `docs_ready: true`
- 追記推奨はあるが、いずれも非ブロッカー。

### 残リスク

- PR本文で webhook endpoint を説明する際、Issue本文の旧表記をそのまま転載すると混乱が再発する。
- 本番導入時に `GITHUB_WEBHOOK_SECRET` の設定先が README 等にないため、運用者が環境変数を見落とす可能性がある。

### 更新した作業ログパス

`docs/logs/28/worklog.md`
