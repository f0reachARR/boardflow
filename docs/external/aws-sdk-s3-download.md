# S3 オブジェクトダウンロード (Rust + aws-sdk-s3)

対象Issue: #7

## 要約

Import Worker は staging bucket から ZIP bundle をダウンロードする必要がある。`aws-sdk-s3` の `get_object` API で `ByteStream` を取得し、`.collect()` でインメモリの `AggregatedBytes` に変換するのが推奨パターン。既存の presigned URL 生成で使用している S3 クライアント設定（MinIO 互換カスタムエンドポイント含む）をそのまま流用できる。

## 確認した情報

### 推奨クレート

| クレート | バージョン | 備考 |
|---|---|---|
| `aws-sdk-s3` | `"1"` + `features = ["behavior-version-latest"]` | workspace Cargo.toml に追加済み |
| `aws-config` | `"1"` + `features = ["behavior-version-latest"]` | workspace Cargo.toml に追加済み |

追加のクレート不要。workspace 依存関係にすでに存在する。

### S3 クライアント設定

既存の `docs/external/aws-sdk-s3-presigned-url.md` で調査済みの設定をそのまま使用可能。
MinIO 互換のカスタムエンドポイント設定（`force_path_style(true)`）が必要。

環境変数ベース:
- `AWS_REGION` / カスタム region
- `AWS_ACCESS_KEY_ID`
- `AWS_SECRET_ACCESS_KEY`
- カスタムエンドポイント URL（MinIO 用）

### get_object によるダウンロード

```rust
use aws_sdk_s3::Client as S3Client;

pub async fn download_object(
    client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    // ByteStream → AggregatedBytes → Bytes → Vec<u8>
    let data = resp.body.collect().await?;
    Ok(data.into_bytes().to_vec())
}
```

### ByteStream の操作方法

`GetObjectOutput.body` は `ByteStream` 型。主な消費方法:

1. **`.collect()`** — 全データをメモリに読み込み `AggregatedBytes` を返す。`AggregatedBytes` は非連続バッファだが `.into_bytes()` で `bytes::Bytes` に変換可能。ZIP展開にはこれが最適。
2. **`.try_next()`** — ストリーミング処理。SHA256 ハッシュ計算をストリーミングで行う場合に有用。
3. **`.into_async_read()`** — `tokio::io::AsyncBufRead` に変換。ファイル書き出し向き。

### SHA256 検証付きダウンロードパターン

Worker は bundle_sha256 との一致を検証する必要がある。ストリーミングハッシュと一括読み込みを組み合わせるパターン:

```rust
use sha2::{Sha256, Digest};
use aws_sdk_s3::Client as S3Client;

pub async fn download_and_verify(
    client: &S3Client,
    bucket: &str,
    key: &str,
    expected_sha256: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let data = resp.body.collect().await?.into_bytes();

    // SHA256 検証
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual = format!("{:x}", hasher.finalize());

    if actual != expected_sha256 {
        return Err(format!(
            "SHA256 mismatch: expected={}, actual={}",
            expected_sha256, actual
        ).into());
    }

    Ok(data.to_vec())
}
```

### サイズ検証

`GetObjectOutput` には `content_length()` メソッドがあり、ダウンロード前にサイズチェックが可能:

```rust
let resp = client.get_object().bucket(bucket).key(key).send().await?;
let content_length = resp.content_length().unwrap_or(0);

if content_length > MAX_BUNDLE_SIZE as i64 {
    return Err("bundle exceeds maximum size".into());
}
```

### エラーハンドリング

主要なエラー型:
- `SdkError<GetObjectError>` — S3 API エラー（NoSuchKey, NoSuchBucket, AccessDenied）
- `ByteStreamError` — ストリーム読み取りエラー（ネットワーク断など）

```rust
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::error::SdkError;

match client.get_object().bucket(b).key(k).send().await {
    Ok(resp) => { /* 処理 */ }
    Err(SdkError::ServiceError(err)) => match err.err() {
        GetObjectError::NoSuchKey(_) => { /* オブジェクト未存在 */ }
        _ => { /* その他のサービスエラー */ }
    },
    Err(err) => { /* ネットワーク等のエラー */ }
}
```

## BoardFlow への示唆

- S3 クライアントは presigned URL 生成と共有可能。`crates/artifact/` に S3 操作を集約するのが自然。
- S3 クライアント構築は `aws-config` 経由でも `aws-sdk-s3::config::Builder` の直接構築でもよい。現行実装は direct builder + 明示的 credentials provider を採用している。
- ダウンロード → SHA256 検証 → ZIP 展開というパイプラインで、collect() による一括メモリ読み込みが適切。
  bundle_size_bytes の上限値（例: 500MB）を設定し、巨大ファイルを拒否すべき。
- `sha2` クレートは workspace Cargo.toml に `"0.10"` で追加済み。

## 採用/不採用判断

**採用**: `aws-sdk-s3` の `get_object` + `ByteStream::collect()` パターンを採用。

## 制約とpitfall

- `collect()` は全データをメモリに読み込むため、バンドルサイズ上限の設定が必須
- MinIO は `force_path_style(true)` 必須（既知）
- `ByteStream::collect()` の `AggregatedBytes` は非連続バッファ。`into_bytes()` で `Bytes` に変換すると連続メモリにコピーされる点に注意
- ネットワークエラー時のリトライは AWS SDK のデフォルトリトライポリシーに任せられる

## 未解決の疑問

- bundle_size_bytes の具体的上限値（500MB? 1GB?）— spec.md に記載なし。MVP では保守的に 500MB 程度が妥当か
- staging bucket 名と final bucket 名の環境変数命名規則

## 参照URL

- https://docs.aws.amazon.com/sdk-for-rust/latest/dg/rust_s3_code_examples.html
- https://docs.rs/aws-sdk-s3/latest/aws_sdk_s3/
- https://docs.rs/aws-smithy-types/latest/aws_smithy_types/byte_stream/struct.ByteStream.html
