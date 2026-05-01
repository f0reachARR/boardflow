# Issue #18: Artifact Proxy API実装 — 作業ログ

## Issue概要

- **タイトル**: Artifact Proxy API実装
- **内容**: viewer-sourcesが返すURLの実体を配信するArtifact Proxy APIを実装する。S3からオブジェクトをストリーミングで取得し、クライアントに配信する。

## Issueまでの経緯

- Issue #7 で Import Worker の S3 ダウンロードパターンを調査済み（`docs/external/aws-sdk-s3-download.md`）
- Import Worker は `ByteStream.collect()` でインメモリ読み込み（ZIP 展開のため）
- Artifact Proxy は Import 後の final bucket から個別 artifact をクライアントへストリーミング配信する用途
- `docs/backend/api.md` §4 に Artifact Proxy API の仕様が定義済み
- `viewer-sources` API（§3.6）が短命 proxy URL を返し、proxy API がそれを受ける構成

## ユーザー要望

docs以下の仕様に基づいてアプリケーションを一通り実装する。本Issueでは Artifact Proxy API を対象とする。

## 調査フェーズ（2026-05-01）

### 調査対象

1. axum 0.8 でのストリーミングレスポンス構築方法
2. aws-sdk-s3 ByteStream → axum Body 変換パターン
3. Content-Type / Content-Disposition / Content-Length ヘッダ設定
4. セキュリティヘッダ（nosniff, CSP, CORS）

### 調査結果

#### axum Body 構築

- `Body::new(impl http_body::Body)` — http-body 1.0 実装を直接ラップ
- `Body::from_stream(TryStream)` — Stream からBody構築
- axum 0.8 は http-body 1.0 を使用

#### S3 ByteStream → axum Body

- `ByteStream.into_inner()` → `SdkBody` → `Body::new(sdk_body)` が推奨パターン
- `SdkBody` は http-body 1.0 を native 実装（aws-smithy-types v1.4.7 で確認）
- Cargo.lock で `http-body 0.4.6` と `http-body 1.0.1` が共存、互換性確認済み
- `collect()` パターンは proxy では非推奨（メモリ効率）

#### ヘッダ設定

- `Response::builder()` でカスタムヘッダ付きレスポンス構築
- `GetObjectOutput.content_length()` → `Option<i64>` で Content-Length 取得可能
- `GetObjectOutput.content_type()` → `Option<&str>` で Content-Type 取得可能
- DB の artifact metadata を優先し、S3 メタデータはフォールバック

#### セキュリティ

- `X-Content-Type-Options: nosniff` 必須
- `Content-Security-Policy` は artifact 種別で分岐（画像/PDF/HTML/ZIP）
- `Access-Control-Allow-Origin` は app domain に限定
- iBOM HTML は `script-src` を許可しつつ iframe sandbox で制御

### 成果物

- `docs/external/axum-s3-streaming-proxy.md` を作成

### 参照URL

- https://docs.rs/axum/latest/axum/body/struct.Body.html
- https://docs.rs/aws-smithy-types/latest/aws_smithy_types/byte_stream/struct.ByteStream.html
- https://docs.rs/aws-smithy-types/latest/aws_smithy_types/body/struct.SdkBody.html
- https://github.com/awslabs/aws-sdk-rust/discussions/989
- https://github.com/awslabs/aws-sdk-rust/issues/1046
- https://github.com/awslabs/aws-sdk-rust/issues/1243
- https://github.com/awslabs/aws-sdk-rust/issues/977

## 結論ステータス

**`implementation_required`**

調査により、axum 0.8 + aws-sdk-s3 v1 でのストリーミング proxy 実装に必要な知見は揃った。追加クレートは不要で、既存の依存関係で実装可能。

## 残リスク

1. `Body::new(SdkBody)` がコンパイルで型エラーになる可能性（bytes クレートバージョン不整合）。その場合は `Body::from_stream()` にフォールバック。

---

## 実装計画フェーズ（2026-05-01）

### コード分析結果

#### `boardflow_domain::models::artifact::Artifact` のフィールド

```rust
pub struct Artifact {
    pub id: Uuid,
    pub board_run_id: Uuid,
    pub r#type: String,           // "kicad_pro", "kicad_sch", "ibom_html", "gerber_zip" 等
    pub status: ArtifactStatus,   // Available | Missing | Failed | Skipped
    pub filename: Option<String>,
    pub source_path: Option<String>,
    pub logical_name: Option<String>,
    pub content_type: Option<String>,  // "application/pdf", "image/svg+xml" 等
    pub storage_key: Option<String>,   // S3 key
    pub sha256: Option<String>,
    pub size_bytes: Option<i64>,
    pub status_reason: Option<String>,
    pub error_message: Option<String>,
    pub source_bundle_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
```

#### `boardflow_db::queries::artifact` の利用可能な関数

- `list_by_board_run(executor, board_run_id)` → `Vec<Artifact>`
- `insert(executor, ...)` → `Artifact`
- **`find_by_id` は未実装** — 新規追加が必要

#### ルーティング方式

- `/api/v1` 配下は `OpenApiRouter` + `routes!` マクロ
- proxy は仕様上 `/proxy/artifacts/{artifact_id}` で `/api/v1` 外
- `lib.rs` で `.route()` を手動追加するパターンで登録（openapi.json と同様）

#### Extension に不足しているもの

- `minio_bucket_final` (String) — proxy handler が S3 バケット名を知る必要がある
- `app_origin` は現状不要（MVP ではプロキシが同一オリジンから配信されるため CORS ヘッダは省略可能。将来追加時に Extension を追加する）

---

### 実装計画

#### 目的

`GET /proxy/artifacts/{artifact_id}?token=...` エンドポイントを実装し、viewer-sources が返す短命 URL 経由で S3 上の artifact をストリーミング配信する。

#### 非目的

- CORS ヘッダの完全実装（MVP では同一ドメイン配信を想定。将来 Issue で追加）
- キャッシュ制御（ETag / If-None-Match）
- Range リクエスト対応
- レート制限

#### 受け入れ条件

1. 有効な token で `/proxy/artifacts/art_{uuid}?token=...` にアクセスすると、S3 から artifact がストリーミング配信される
2. Content-Type が artifact metadata から設定される
3. `X-Content-Type-Options: nosniff` ヘッダが付与される
4. iframe 用 artifact (ibom_html) には制限付き CSP ヘッダが付与される
5. 無効/期限切れ token は 401 を返す
6. token の artifact_id と URL の artifact_id が不一致の場合は 401 を返す
7. 存在しない artifact / status が available でない artifact は 404 を返す
8. S3 client が未設定の場合は 503 を返す
9. S3 から取得失敗時は 502 を返す

#### 詳細要件

| 項目 | 仕様 |
|---|---|
| パス | `GET /proxy/artifacts/{artifact_id}?token=...` |
| artifact_id 形式 | `art_` + UUID v7 |
| token 検証 | `verify_artifact_token()` → (artifact_id, user_id) |
| token artifact_id 一致確認 | URL の artifact_id == token 内 artifact_id |
| artifact 取得 | DB から `find_by_id` で取得、status == Available を確認 |
| S3 取得 | `get_object(bucket=minio_bucket_final, key=artifact.storage_key)` |
| Content-Type | DB の `artifact.content_type` 優先、なければ `application/octet-stream` |
| Content-Length | S3 GetObjectOutput の `content_length` |
| X-Content-Type-Options | `nosniff` (全 artifact) |
| Content-Security-Policy | artifact type で分岐（後述） |
| Content-Disposition | `inline` (表示用)、ZIP は `attachment` |
| Body | `Body::new(resp.body.into_inner())` でストリーミング |

##### CSP 分岐ルール

| artifact type パターン | CSP |
|---|---|
| `ibom_html` | `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'` |
| `*_svg` / `image/*` | `default-src 'none'; img-src 'self'; style-src 'unsafe-inline'` |
| `schematic_pdf` | `default-src 'none'; plugin-types application/pdf` |
| `gerber_zip` / `*_zip` | `default-src 'none'` |
| その他 | `default-src 'none'` |

#### 影響範囲

| ファイル | 変更種別 |
|---|---|
| `crates/db/src/queries/artifact.rs` | 追加: `find_by_id` 関数 |
| `crates/api/src/routes/proxy.rs` | **新規**: proxy handler |
| `crates/api/src/routes/mod.rs` | 追加: `pub mod proxy;` |
| `crates/api/src/lib.rs` | 追加: proxy route 登録 + `FinalBucket` Extension |
| `crates/api/tests/proxy_test.rs` | **新規**: proxy API テスト |

#### 設計方針

1. **ルーティング**: `/proxy/artifacts/:artifact_id` を `axum::routing::get()` で手動登録（OpenApiRouter 外）
2. **Extension**: `FinalBucket(String)` を新しい newtype として定義し Extension に追加
3. **ストリーミング**: `Body::new(SdkBody)` パターン採用。コンパイルエラー時は `Body::from_stream()` にフォールバック
4. **エラー**: 既存の `AppError` を使い JSON エラーレスポンスを返す
5. **テスト**: S3 依存のため、統合テストでは MinIO を使う or モック。まずは token 検証 / DB 検証のユニットテスト + S3 なしでの 503 テストを優先

---

### 新規作成ファイル一覧

1. `crates/api/src/routes/proxy.rs` — proxy handler 実装
2. `crates/api/tests/proxy_test.rs` — proxy API テスト

### 既存ファイルの変更一覧

1. **`crates/db/src/queries/artifact.rs`**
   - `find_by_id(executor, id: Uuid) -> Result<Option<Artifact>, sqlx::Error>` を追加

2. **`crates/api/src/routes/mod.rs`**
   - `pub mod proxy;` を追加

3. **`crates/api/src/lib.rs`**
   - `FinalBucket(pub String)` newtype 定義を追加
   - `create_app_with_config` に `final_bucket: Option<String>` パラメータ追加（or 環境変数から取得）
   - `.route("/proxy/artifacts/:artifact_id", get(routes::proxy::proxy_artifact))` 登録
   - `Extension(FinalBucket(...))` layer 追加

---

### 実装ステップ（TDD: テスト先行）

#### Step 1: DB 層 — `find_by_id` 追加

```rust
// crates/db/src/queries/artifact.rs に追加
pub async fn find_by_id(
    executor: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    id: Uuid,
) -> Result<Option<Artifact>, sqlx::Error> {
    sqlx::query_as::<_, Artifact>("SELECT * FROM artifacts WHERE id = $1")
        .bind(id)
        .fetch_optional(executor)
        .await
}
```

#### Step 2: テスト作成（proxy_test.rs）

テストケース:
1. `test_proxy_missing_token_returns_401` — token パラメータなしで 401
2. `test_proxy_invalid_token_returns_401` — 不正 token で 401
3. `test_proxy_artifact_id_mismatch_returns_401` — token 内の artifact_id と URL 不一致で 401
4. `test_proxy_artifact_not_found_returns_404` — DB に artifact なしで 404
5. `test_proxy_artifact_not_available_returns_404` — status が available でない artifact で 404
6. `test_proxy_no_s3_client_returns_503` — S3 client なしで 503
7. `test_proxy_success_streams_artifact` — MinIO ありで正常ストリーミング（統合テスト環境依存）
8. `test_proxy_response_headers` — nosniff, CSP, Content-Type 確認

#### Step 3: proxy handler 実装

`crates/api/src/routes/proxy.rs`:
- `parse_artifact_id(s: &str) -> Option<Uuid>` ヘルパー
- `csp_for_artifact(artifact_type: &str) -> &'static str` ヘルパー
- `content_disposition_for_artifact(artifact_type: &str, filename: Option<&str>) -> String` ヘルパー
- `proxy_artifact` handler 関数

#### Step 4: ルーティング登録

`crates/api/src/lib.rs` に proxy route + FinalBucket Extension を追加

#### Step 5: 統合テスト実行・修正

---

### テスト戦略

| レイヤ | テスト内容 | 外部依存 |
|---|---|---|
| Unit | `csp_for_artifact` / `content_disposition_for_artifact` / `parse_artifact_id` | なし |
| Integration (DB) | token 検証 → DB 検索 → 404/401 レスポンス | PostgreSQL |
| Integration (S3) | 正常ストリーミング + ヘッダ確認 | PostgreSQL + MinIO |

DB 依存テストは既存の `setup_pool()` パターンに従い `DATABASE_URL` 未設定時はスキップ。
S3 依存テストは `MINIO_ENDPOINT` 未設定時はスキップ。

---

### ドキュメント更新対象

- `docs/logs/18/worklog.md` — 本ログ（計画・実装記録）
- `docs/backend/api.md` — 実装後に仕様との差分があれば更新

---

### 実装可否判断

**`implementation_required: true`** / **`implementation_ready: true`**

全ての技術的疑問が解消されており、追加調査・ユーザー質問は不要。既存コードベース・調査結果に基づいて即座に実装着手可能。

---

### 未解決の疑問

なし。調査で全て解消済み。

- iBOM CSP: MVP では `script-src 'unsafe-inline'; style-src 'unsafe-inline'` で十分。外部 CDN 依存が判明した場合は後続 Issue で調整。
- `Body::new(SdkBody)` の互換性: コンパイルで確認し、問題あれば `from_stream` にフォールバック。

---

### 更新した作業ログパス

`docs/logs/18/worklog.md`
2. iBOM HTML の CSP 設定は実際の iBOM 出力を確認して調整が必要。
3. artifact token の生成/検証ロジックは既存の `crates/api/src/artifact_token.rs` の設計に依存。
