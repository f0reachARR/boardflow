# Axum + S3 ストリーミング Proxy パターン

対象Issue: #18

## 要約

Artifact Proxy API では、S3 の `get_object` で取得した `ByteStream` を axum の HTTP レスポンスとしてストリーミング配信する。`aws-sdk-s3` v1.119.0 の `SdkBody` は `http-body 1.0` を native 実装しており、axum 0.8 の `Body::new()` で直接ラップできる。全データをメモリに載せずにクライアントへ中継可能。Content-Type / Content-Length は `GetObjectOutput` のメタデータから取得し、セキュリティヘッダ（nosniff, CSP）を手動で付与する。

## 確認した情報

### 1. axum 0.8 の Body 構築方法

`axum::body::Body` は以下の方法で構築できる。

| メソッド | 用途 |
|---|---|
| `Body::new(body)` | `http_body::Body<Data = Bytes>` を実装する型をラップ |
| `Body::from_stream(stream)` | `TryStream<Ok: Into<Bytes>, Error: Into<BoxError>>` からBody構築 |
| `Body::from(bytes)` | `Bytes` / `&'static [u8]` / `String` / `Vec<u8>` からBody構築 |

ストリーミング proxy では `Body::new()` または `Body::from_stream()` を使う。

**参照**: https://docs.rs/axum/latest/axum/body/struct.Body.html

### 2. aws-sdk-s3 ByteStream → axum Body 変換

#### 推奨パターン: `Body::new(SdkBody)`

`ByteStream.into_inner()` で `SdkBody` を取得し、`axum::body::Body::new()` でラップする。

```rust
use axum::body::Body;
use aws_sdk_s3::Client as S3Client;

let resp = s3_client
    .get_object()
    .bucket(bucket)
    .key(key)
    .send()
    .await?;

let sdk_body = resp.body.into_inner();
let body = Body::new(sdk_body);
```

**理由**: `SdkBody` は `http-body 1.0::Body<Data = Bytes>` を native 実装（aws-smithy-types v1.4.7 で確認）。`Body::new()` は `impl http_body::Body` を直接受け取るため、Stream 変換のオーバーヘッドがない。

#### 代替パターン: `Body::from_stream(ByteStream)`

`ByteStream` は `futures::Stream<Item = Result<Bytes, Error>>` を実装しており、`Body::from_stream()` で直接渡せる可能性がある。ただし `ByteStream` の `Stream` trait 実装は明示的にドキュメント化されておらず、`poll_next` メソッドの存在のみ確認。

```rust
// ByteStream が futures::Stream を実装している場合
let body = Body::from_stream(resp.body);
```

`Body::new(SdkBody)` の方が型変換経路が明確で安全。

#### 非推奨: `collect()` によるインメモリ変換

```rust
let data = resp.body.collect().await?.into_bytes();
let body = Body::from(data);
```

全データをメモリに載せるため、大きな artifact（Gerber ZIP、PDF 等）では非推奨。Import Worker（Issue #7）では ZIP 全体を処理する必要があるため `collect()` を使うが、Proxy API ではストリーミングが適切。

### 3. http-body バージョン互換性

BoardFlow の Cargo.lock 確認結果:

| クレート | バージョン |
|---|---|
| `axum` | `0.8` (http-body 1.0 使用) |
| `aws-sdk-s3` | `1.119.0` |
| `aws-smithy-types` | `1.4.7` |
| `http-body` | `0.4.6` と `1.0.1` が共存 |

`aws-smithy-types` v1.4.7 は `http-body 0.4.6` と `http-body 1.0.1` の両方に依存し、`SdkBody` が両バージョンの `Body` trait を実装。axum 0.8 は `http-body 1.0` を使用するため、`Body::new(SdkBody)` は型レベルで互換。

**既知の注意点**: GitHub Issue awslabs/aws-sdk-rust#1243 で axum 0.8 + aws-sdk-s3 の http-body バージョン不整合が報告されている（2025年1月）。ただし、これは特定バージョンの組み合わせで発生し、aws-smithy-types が http-body 1.0 の依存を持つ現行バージョンでは解消済み。BoardFlow の Cargo.lock で `http-body-util` が含まれていることも互換性を裏付ける。コンパイルで問題が出た場合は `bytes` クレートのバージョン統一を確認する。

### 4. GetObjectOutput のメタデータ

`GetObjectOutput` から以下のフィールドを取得可能:

| フィールド | 型 | 用途 |
|---|---|---|
| `content_length()` | `Option<i64>` | Content-Length ヘッダ設定 |
| `content_type()` | `Option<&str>` | Content-Type ヘッダ設定 |
| `content_disposition()` | `Option<&str>` | Content-Disposition ヘッダ設定 |
| `e_tag()` | `Option<&str>` | ETag ヘッダ（キャッシュ制御） |
| `last_modified()` | `Option<DateTime>` | Last-Modified ヘッダ |

**Content-Length**: ストリーミング配信でも `GetObjectOutput.content_length()` から取得可能。HTTP レスポンスの Content-Length に設定することで、クライアントが進捗表示できる。

**Content-Type**: Import Worker が artifact metadata として DB に保存した `content_type` を使用する。S3 オブジェクトの Content-Type は Upload 時に設定済みの前提。DB の artifact metadata と S3 の Content-Type の両方を参照し、DB を優先する。

### 5. レスポンスヘッダ設定パターン

axum でカスタムヘッダ付きレスポンスを構築する方法:

#### パターン A: `Response::builder()` + `Body`

```rust
use axum::response::Response;
use axum::body::Body;
use http::header;

let resp = Response::builder()
    .header(header::CONTENT_TYPE, content_type)
    .header(header::CONTENT_LENGTH, content_length)
    .header("X-Content-Type-Options", "nosniff")
    .header("Content-Security-Policy", "default-src 'none'")
    .body(body)
    .unwrap();
```

#### パターン B: タプル `(StatusCode, HeaderMap, Body)`

```rust
use axum::http::{StatusCode, HeaderMap, HeaderValue};

let mut headers = HeaderMap::new();
headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(content_type)?);
headers.insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));

(StatusCode::OK, headers, body)
```

Artifact Proxy では多数のヘッダを設定する必要があるため、**パターン A** (`Response::builder()`) が適切。

### 6. セキュリティヘッダ

仕様（docs/backend/api.md §4）で要求されるヘッダ:

| ヘッダ | 値 | 目的 |
|---|---|---|
| `X-Content-Type-Options` | `nosniff` | MIME sniffing 防止 |
| `Content-Security-Policy` | artifact種別で分岐 | XSS/injection 防止 |
| `Access-Control-Allow-Origin` | app domain のみ | CORS 制限 |
| `Access-Control-Allow-Methods` | `GET` | 許可メソッド制限 |
| `Vary` | `Origin` | キャッシュ分離 |
| `Referrer-Policy` | `no-referrer` | Referer 漏洩防止 |
| `Content-Disposition` | `inline` or `attachment` | ブラウザ表示/DL制御 |

#### Content-Security-Policy の分岐

| artifact 種別 | CSP |
|---|---|
| 画像 (SVG/PNG) | `default-src 'none'; frame-ancestors 'none'` |
| PDF | `default-src 'none'; frame-ancestors 'none'` |
| HTML (iBOM) | `sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; frame-ancestors <app_domain>` |
| KiCad source | `default-src 'none'; frame-ancestors 'none'` |
| ZIP / ダウンロード | `default-src 'none'; frame-ancestors 'none'` |

iBOM は JavaScript を実行する HTML であるため、CSP `sandbox allow-scripts` で unique origin 化し、`frame-ancestors` で埋め込み元を app domain に制限する。非 iframe artifact は `frame-ancestors 'none'` で iframe 埋め込みを拒否する。

### 7. 完全な実装スケルトン

```rust
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use aws_sdk_s3::Client as S3Client;

pub async fn proxy_artifact(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    Query(params): Query<ProxyParams>,
) -> Result<Response, AppError> {
    // 1. token 検証（短命トークン、artifact_id / user / expiry 紐づき）
    let artifact = validate_proxy_token(&state, &artifact_id, &params.token).await?;

    // 2. S3 から get_object
    let resp = state.s3_client
        .get_object()
        .bucket(&state.final_bucket)
        .key(&artifact.storage_key)
        .send()
        .await
        .map_err(|_| AppError::NotFound)?;

    // 3. メタデータ取得
    let content_type = artifact.content_type.as_deref()
        .or(resp.content_type().as_deref())
        .unwrap_or("application/octet-stream");
    let content_length = resp.content_length().unwrap_or_default();

    // 4. ストリーミング Body 構築
    let sdk_body = resp.body.into_inner();
    let body = Body::new(sdk_body);

    // 5. レスポンス構築
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, content_length)
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", csp_for_artifact(&artifact))
        .header(
            "Access-Control-Allow-Origin",
            &state.app_origin,
        )
        .body(body)
        .unwrap();

    Ok(response)
}
```

## BoardFlow への示唆

1. **追加クレート不要**: `axum`, `aws-sdk-s3`, `futures` は workspace 依存に存在。http-body 変換に追加クレートは不要。
2. **ルーティング**: 仕様に基づき `/proxy/artifacts/{artifact_id}` を別ルートとして設定。API prefix (`/api/v1`) の外に置く。
3. **トークン検証**: query parameter `?token=...` で短命トークンを受け取り、HMAC 署名検証する。viewer-sources API でトークン生成、proxy API でトークン検証。
4. **エラーハンドリング**: token 無効/期限切れ → 401、artifact 未存在/非 available → 404、storage 未設定 → 500、S3 障害 → 500。
5. **artifact metadata**: DB の artifact テーブルに `content_type`, `size_bytes`, `storage_key` が保存済みの前提で、S3 レスポンスのメタデータより DB を優先。

## 採用/不採用判断

| 項目 | 判断 | 理由 |
|---|---|---|
| `Body::new(SdkBody)` パターン | **採用** | http-body 1.0 native 実装で直接変換可能、オーバーヘッド最小 |
| `Body::from_stream()` パターン | 代替候補 | ByteStream の Stream trait 実装が明確でない場合のフォールバック |
| `collect()` パターン | **不採用** | proxy 用途ではメモリ効率が悪い |
| `Response::builder()` ヘッダ設定 | **採用** | 多数のヘッダを設定する場合に最も読みやすい |
| CSP artifact 種別分岐 | **採用** | iBOM HTML 等 script 実行が必要な artifact への対応 |

## 制約と pitfall

1. **http-body バージョン不整合**: `bytes` クレートのバージョンが axum と aws-sdk-s3 で異なると `Body::new(SdkBody)` がコンパイルエラーになる。`cargo tree -d | grep bytes` で確認し、workspace で `bytes` バージョンを統一する。
2. **SdkBody の Sync**: `SdkBody` は `Send + Sync` だが、ストリーミング中の `SdkBody` は single-use。retry 不可。Proxy 用途では retry は不要だが、S3 エラー時のレスポンスは別途ハンドリングが必要。
3. **Content-Length の i64 → header 変換**: `GetObjectOutput.content_length()` は `Option<i64>` を返す。負値は通常ないが、ヘッダ設定時に `u64` 変換が必要。
4. **大きな artifact のタイムアウト**: ストリーミングなので axum/hyper のレスポンスタイムアウト設定に注意。デフォルトではレスポンス全体の送信完了までタイムアウトしない。
5. **SVG の Content-Type**: SVG は `image/svg+xml` だが JavaScript を含む可能性がある。CSP `default-src 'none'; img-src 'self'` と nosniff で軽減するが、信頼できない SVG の直接レンダリングはリスクがある。仕様では proxy 経由の配信で制御する方針。
6. **S3 エラー時のストリーミング中断**: S3 が途中でエラーを返した場合、クライアントは不完全なレスポンスを受け取る。Content-Length が設定されていれば、クライアント側で不完全さを検出可能。

## 未解決の疑問

1. **ByteStream の futures::Stream trait 実装**: `ByteStream` が `futures::Stream` trait を正式に実装しているかドキュメントで明確でない。`poll_next` メソッドは存在するが、trait impl の有無はコンパイル時に確認が必要。`Body::new(SdkBody)` を主パターンとすることで回避。
2. **iBOM の CSP 詳細**: iBOM HTML が必要とする外部リソース（fonts, CDN等）の詳細が未確認。実装時に実際の iBOM 出力を確認して CSP を調整する必要がある。
3. **artifact token の生成方式**: HMAC ベースか JWT ベースか、viewer-sources API 側の設計に依存。既存の `artifact_token.rs` を確認する。

## 参照URL

- [axum::body::Body docs](https://docs.rs/axum/latest/axum/body/struct.Body.html)
- [aws-smithy-types ByteStream docs](https://docs.rs/aws-smithy-types/latest/aws_smithy_types/byte_stream/struct.ByteStream.html)
- [aws-smithy-types SdkBody docs](https://docs.rs/aws-smithy-types/latest/aws_smithy_types/body/struct.SdkBody.html)
- [awslabs/aws-sdk-rust#989 - axum 0.7 Streaming from body to S3](https://github.com/awslabs/aws-sdk-rust/discussions/989)
- [awslabs/aws-sdk-rust#1046 - SdkBody implements http-body 1.0](https://github.com/awslabs/aws-sdk-rust/issues/1046)
- [awslabs/aws-sdk-rust#1243 - http-body version incompatible](https://github.com/awslabs/aws-sdk-rust/issues/1243)
- [awslabs/aws-sdk-rust#977 - tracking hyper/http 1.0 support](https://github.com/awslabs/aws-sdk-rust/issues/977)
- [Rust Users Forum - Streaming S3 to Axum](https://users.rust-lang.org/t/axum-server-handling-body-stream/121081)
- [StackOverflow - axum response headers](https://stackoverflow.com/questions/76557812/how-to-set-a-response-header-in-an-axum-handler)
