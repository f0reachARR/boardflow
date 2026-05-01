# S3 互換 Presigned URL 生成 (Rust + aws-sdk-s3)

対象Issue: #5

## 要約

BoardRun作成APIのレスポンスで `artifact_bundle.upload_url` として staging bucket への presigned PUT URL を返す必要がある。Rust では `aws-sdk-s3` クレートが公式 AWS SDK であり、presigned URL 生成に対応している。MinIO互換のカスタムエンドポイント設定も `aws-config` + `force_path_style(true)` で可能。

## 確認した情報

### 推奨クレート

| クレート | 最新安定版 | 推奨指定 |
|---|---|---|
| `aws-sdk-s3` | 1.131.0+ (2026-04時点) | `"1"` + `features = ["behavior-version-latest"]` |
| `aws-config` | 1.8.16 (2026-04時点) | `"1"` + `features = ["behavior-version-latest"]` |

代替として `rust-s3` (durch/rust-s3) もあるが、AWS公式SDKの方が長期メンテナンス・互換性面で推奨。

### Cargo.toml 追加

```toml
[workspace.dependencies]
aws-config = { version = "1", features = ["behavior-version-latest"] }
aws-sdk-s3 = { version = "1", features = ["behavior-version-latest"] }
```

### MinIO 互換カスタムエンドポイント設定

```rust
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::Builder as S3ConfigBuilder;

pub async fn create_s3_client(
    endpoint_url: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> aws_sdk_s3::Client {
    let creds = aws_sdk_s3::config::Credentials::new(
        access_key,
        secret_key,
        None, // session token
        None, // expiry
        "boardflow-static",
    );

    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(endpoint_url)
        .credentials_provider(creds)
        .region(aws_config::Region::new(region.to_owned()))
        .load()
        .await;

    let s3_config = S3ConfigBuilder::from(&config)
        .force_path_style(true) // MinIO requires path-style
        .build();

    aws_sdk_s3::Client::from_conf(s3_config)
}
```

重要: MinIO は virtual-hosted style (`bucket.endpoint`) に対応しないため、`force_path_style(true)` が必須。

### Presigned PUT URL 生成

```rust
use aws_sdk_s3::presigning::PresigningConfig;
use std::time::Duration;

pub async fn generate_presigned_put_url(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    object_key: &str,
    expires_in_secs: u64,
) -> Result<String, aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::put_object::PutObjectError>> {
    let expires_in = Duration::from_secs(expires_in_secs);
    let presigning_config = PresigningConfig::expires_in(expires_in)
        .expect("expiration within one week");

    let presigned_request = client
        .put_object()
        .bucket(bucket)
        .key(object_key)
        .presigned(presigning_config)
        .await?;

    Ok(presigned_request.uri().to_string())
}
```

### 環境変数マッピング

| 環境変数 | 用途 |
|---|---|
| `MINIO_ENDPOINT` | S3互換エンドポイントURL (例: `http://localhost:9000`) |
| `MINIO_ACCESS_KEY` | アクセスキー |
| `MINIO_SECRET_KEY` | シークレットキー |
| `MINIO_BUCKET_STAGING` | staging bucket名 |
| `MINIO_REGION` | リージョン (MinIOでは任意、例: `us-east-1`) |

### 有効期限の設計

- BoardRun作成API仕様では `expires_at` を返す
- 推奨有効期限: 1時間 (3600秒) — zip bundle upload に十分な時間
- `PresigningConfig` の上限は7日 (604800秒)

## BoardFlow への示唆

- `crates/artifact/` に S3 クライアント初期化と presigned URL 生成ロジックを配置する
- `AppState` に S3 client を保持し、DI する
- BoardRun作成時に `staging/runs/{board_run_id}/bundle.zip` の presigned PUT URL を生成して返す
- `expires_at` は生成時刻 + 有効期限から計算して UTC RFC3339 で返す

## 採用/不採用判断

**採用**: `aws-sdk-s3` + `aws-config` を workspace dependencies に追加する。

理由:
- AWS公式メンテナンス、MinIO互換確認済み
- presigned URL生成がSDK組み込み機能として提供されている
- Tokio非同期ランタイムとの親和性が高い
- `force_path_style` でMinIO対応が容易

## 制約とpitfall

- `force_path_style(true)` を忘れるとMinIOで `BucketNotFound` エラーになる
- presigned URLの有効期限は最大7日。それ以上は生成時エラーになる
- `aws-sdk-s3` は STS / IAM 機能を含まないため、静的クレデンシャルの場合は `Credentials::new()` で直接渡す
- `behavior-version-latest` feature を有効にしないと `BehaviorVersion::latest()` が使えない
- MinIOのbucketが事前に存在する必要がある (docker-compose で `createbuckets` サービスを追加する)

## 未解決の疑問

- 本番環境で使用するS3互換サービスの決定 (MinIO自前運用 or クラウドS3 or Cloudflare R2)
- presigned URLの有効期限を設定ファイルから変更可能にするか (MVP では固定値で十分)

## 参照URL

- https://docs.aws.amazon.com/sdk-for-rust/latest/dg/rust_s3_code_examples.html
- https://docs.rs/aws-sdk-s3/latest/aws_sdk_s3/
- https://docs.rs/aws-config/latest/aws_config/
- https://crates.io/crates/aws-sdk-s3
- https://crates.io/crates/aws-config
- https://github.com/awslabs/aws-sdk-rust/issues/390 (force_path_style for MinIO)
