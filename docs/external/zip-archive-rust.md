# ZIP アーカイブ展開 (Rust + zip クレート)

対象Issue: #7

## 要約

Import Worker は S3 からダウンロードした ZIP バンドルをインメモリで展開し、各 artifact ファイルを抽出する必要がある。`zip` クレート (zip-rs/zip2) が Rust のデファクトスタンダードで、`std::io::Cursor` を使ったインメモリ展開に対応している。セキュリティ上、パストラバーサル対策（`enclosed_name()` の使用）とzip bomb対策（展開サイズ制限）が必須。

## 確認した情報

### 推奨クレート

| クレート | 最新安定版 | 推奨指定 |
|---|---|---|
| `zip` | 2.6.x (2026-05時点、crates.io: zip-rs/zip2) | `"2"` |

**重要**: `zip` クレート v2.3.0 でパストラバーサル脆弱性 (CVE-2025-29787) が修正されている。2.3.0 未満はシンボリックリンクを悪用した書き込みが可能。v2.3.0 以上を使用すること。

BoardFlow では ZIP をファイルシステムに展開するのではなくインメモリで読み取るため、`extract()` 系メソッドは使用しない。それでも `enclosed_name()` による名前のサニタイズは必須。

### Cargo.toml 追加

```toml
[workspace.dependencies]
zip = "2"
```

`crates/artifact/Cargo.toml`:
```toml
[dependencies]
zip = { workspace = true }
```

### インメモリ ZIP 展開パターン

```rust
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub struct ExtractedFile {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn extract_zip_in_memory(
    zip_bytes: &[u8],
    max_total_size: u64,
) -> Result<Vec<ExtractedFile>, ZipExtractError> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor)?;

    // zip bomb 対策: decompressed_size() で事前チェック
    if let Some(decompressed) = archive.decompressed_size() {
        if decompressed > max_total_size as u128 {
            return Err(ZipExtractError::TooLarge {
                decompressed_size: decompressed,
                max_size: max_total_size,
            });
        }
    }

    let mut files = Vec::new();
    let mut total_extracted: u64 = 0;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        // ディレクトリはスキップ
        if file.is_dir() {
            continue;
        }

        // パストラバーサル対策: enclosed_name() を使用
        let name = match file.enclosed_name() {
            Some(path) => path.to_string_lossy().into_owned(),
            None => {
                // 不正なパス（../ や絶対パス）を含むエントリは拒否
                return Err(ZipExtractError::InvalidPath(
                    file.name().to_string(),
                ));
            }
        };

        // 個別ファイルサイズ制限
        let uncompressed = file.size();
        total_extracted += uncompressed;
        if total_extracted > max_total_size {
            return Err(ZipExtractError::TooLarge {
                decompressed_size: total_extracted as u128,
                max_size: max_total_size,
            });
        }

        // ファイル読み込み（サイズ上限付き）
        let mut data = Vec::with_capacity(uncompressed as usize);
        file.read_to_end(&mut data)?;

        files.push(ExtractedFile { name, data });
    }

    Ok(files)
}
```

### `enclosed_name()` について

`ZipFile::enclosed_name()` は、パス名を安全にサニタイズして返す。以下の場合に `None` を返す:
- 絶対パス (`/etc/passwd`)
- パストラバーサル (`../../../etc/passwd`)
- その他のプラットフォーム固有の危険なパス

`zip` クレートの `extract()` メソッドも内部で `enclosed_name()` を使用している。
BoardFlow ではインメモリ展開のため `extract()` は使わないが、`enclosed_name()` を直接呼び出して安全性を確保する。

### zip bomb 対策

複数の防御層:

1. **事前チェック**: `ZipArchive::decompressed_size()` で展開後の総サイズを推定（central directory ベース）
2. **展開中チェック**: 累計展開サイズを追跡し、上限超過で中断
3. **個別ファイルチェック**: `file.size()` で個別ファイルの展開サイズを確認
4. **圧縮率チェック** (オプション): 異常に高い圧縮率（例: 1000:1 以上）のエントリを拒否

```rust
// 圧縮率チェック例
let compressed = file.compressed_size();
let uncompressed = file.size();
if compressed > 0 && uncompressed / compressed > 1000 {
    return Err(ZipExtractError::SuspiciousCompressionRatio);
}
```

### manifest.json の読み取り

ZIP 内の特定ファイルを名前で取得:

```rust
// manifest.json を先に読み取り
let manifest_data = {
    let mut manifest_file = archive.by_name("manifest.json")?;
    let mut buf = Vec::new();
    manifest_file.read_to_end(&mut buf)?;
    buf
};

let manifest: Manifest = serde_json::from_slice(&manifest_data)?;
```

### エントリの列挙と型判定

```rust
for i in 0..archive.len() {
    let file = archive.by_index(i)?;
    println!(
        "name={}, size={}, compressed={}, is_dir={}",
        file.name(),
        file.size(),
        file.compressed_size(),
        file.is_dir(),
    );
}
```

### `ZipArchive` の制約

- `ZipArchive::new()` は `Read + Seek` トレイトを要求。`Cursor<Vec<u8>>` と `Cursor<&[u8]>` の両方で動作する。
- `by_index()` は `&mut self` を取るため、同時に複数エントリを開くことはできない。ループ内で1つずつ処理する。
- ZIP の暗号化機能（ZipCrypto）はセキュリティ上弱く、BoardFlow では使用しない。

## BoardFlow への示唆

- `crates/artifact/` に ZIP 展開機能を実装する。`zip` クレートの依存を `crates/artifact/Cargo.toml` に追加。
- manifest.json を最初に読み取り、検証後に各 artifact を展開する流れが自然。
- 展開後の各ファイルは `HashMap<String, Vec<u8>>` として保持し、後段の S3 アップロード・DB 登録に渡す。
- `decompressed_size()` による事前チェック + 展開中の累計サイズ追跡の二重防御を推奨。
- max_total_size のデフォルト値は bundle_size_bytes の上限と合わせる（例: 500MB）。

## 採用/不採用判断

**採用**: `zip` クレート v2.x を採用。インメモリ展開 + `enclosed_name()` + サイズ制限の組み合わせで実装。

## 制約とpitfall

- `zip` < 2.3.0 にはパストラバーサル脆弱性 (CVE-2025-29787) あり。必ず 2.3.0 以上を使用
- `enclosed_name()` が `None` を返すエントリは必ず拒否すること
- `decompressed_size()` は central directory ベースの推定値で、悪意あるアーカイブでは不正確な可能性がある。展開中のサイズ追跡も必須
- `by_index()` は `&mut self` のため並列展開不可（MVP では問題なし）
- ZIP64 アーカイブも `zip` v2 でサポートされている
- パスワード付き ZIP は非対応（BoardFlow では不要）

## 未解決の疑問

- manifest.json の具体的なスキーマ定義（spec.md に詳細なし。Issue #7 の実装時に確定が必要）
- ZIP 内のディレクトリ構造の規約（フラット vs ネスト）

## 参照URL

- https://docs.rs/zip/latest/zip/read/struct.ZipArchive.html
- https://crates.io/crates/zip
- https://github.com/zip-rs/zip2
- https://security.snyk.io/vuln/SNYK-RUST-ZIP-9600990 (CVE-2025-29787)
- https://github.com/tenuo-ai/safe_unzip (参考: ZIP セキュリティ専用ライブラリ)
