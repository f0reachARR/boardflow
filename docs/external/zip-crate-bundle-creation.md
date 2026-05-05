# Rust zip クレート v2 でのディレクトリ構造付き ZIP 作成

## 要約

`zip` クレート v2 と `walkdir` を使い、ディレクトリを再帰的に ZIP ファイルに格納するパターン。BoardFlow action-runner の bundle.zip 作成に必要。

## 確認した情報

### 基本的な ZIP 作成

```rust
use std::fs::File;
use std::io::{Write, Read};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn create_zip_from_file(zip_path: &Path, file_path: &Path, archive_name: &str) -> zip::result::ZipResult<()> {
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file(archive_name, options)?;
    let mut buf = Vec::new();
    File::open(file_path)?.read_to_end(&mut buf)?;
    zip.write_all(&buf)?;

    zip.finish()?;
    Ok(())
}
```

### ディレクトリ再帰 ZIP (walkdir 使用)

```rust
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// staging_dir の中身を bundle_path に ZIP 化
/// ZIP 内のパスは staging_dir からの相対パス
fn create_bundle_zip(staging_dir: &Path, bundle_path: &Path) -> anyhow::Result<()> {
    let file = File::create(bundle_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in WalkDir::new(staging_dir) {
        let entry = entry?;
        let path = entry.path();
        let rel_path = path.strip_prefix(staging_dir)?;

        // ルートディレクトリ自体はスキップ
        if rel_path.as_os_str().is_empty() {
            continue;
        }

        // パスをスラッシュ区切りに変換 (Windows 互換)
        let name = rel_path.to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            // ディレクトリエントリ (末尾スラッシュ)
            zip.add_directory(&name, options)?;
        } else {
            // ファイルエントリ
            zip.start_file(&name, options)?;
            let mut buf = Vec::new();
            File::open(path)?.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        }
    }

    zip.finish()?;
    Ok(())
}
```

### 大きなファイルのストリーミング書き込み

```rust
use std::io::copy;

fn create_bundle_zip_streaming(staging_dir: &Path, bundle_path: &Path) -> anyhow::Result<()> {
    let file = File::create(bundle_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in WalkDir::new(staging_dir).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        let rel_path = path.strip_prefix(staging_dir)?;

        if rel_path.as_os_str().is_empty() {
            continue;
        }

        let name = rel_path.to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            zip.add_directory(&name, options)?;
        } else {
            zip.start_file(&name, options)?;
            let mut f = File::open(path)?;
            copy(&mut f, &mut zip)?;
        }
    }

    zip.finish()?;
    Ok(())
}
```

### Gerber/Drill サブセット ZIP

```rust
/// 特定ディレクトリの中身だけを ZIP 化 (gerbers.zip, drill.zip)
fn create_subset_zip(source_dir: &Path, zip_path: &Path) -> anyhow::Result<()> {
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for entry in WalkDir::new(source_dir).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel_path = entry.path().strip_prefix(source_dir)?;
        let name = rel_path.to_string_lossy().replace('\\', "/");

        zip.start_file(&name, options)?;
        let mut f = File::open(entry.path())?;
        std::io::copy(&mut f, &mut zip)?;
    }

    zip.finish()?;
    Ok(())
}
```

### bash 実装との対応

| bash (entrypoint.sh) | Rust |
|---|---|
| `(cd "$gerber_dir" && zip -qr "$gerbers_zip" . 2>/dev/null)` | `create_subset_zip(gerber_dir, gerbers_zip)` |
| `(cd "$drill_dir" && zip -qr "$drill_zip" . 2>/dev/null)` | `create_subset_zip(drill_dir, drill_zip)` |
| `create_bundle_zip "$staging_dir" "$bundle_path"` | `create_bundle_zip(staging_dir, bundle_path)` |

### zip v2 の API 変更点 (v0.6 → v2)

- `FileOptions` → `SimpleFileOptions` にリネーム
- `start_file` の第2引数が `SimpleFileOptions` を直接受け取る
- `add_directory` メソッドが追加
- `finish()` は `ZipResult<W>` を返す (writer を回収可能)

## BoardFlow への示唆

- workspace に既に `zip = "2"` と `walkdir = "2"` が依存にあるため追加不要
- bundle.zip の構造は spec で定義済み (manifest.json, review/, assembly/, fabrication/, checks/, diff/, kicad/)
- `sort_by_file_name()` で決定論的な ZIP 構造を保証 (hash 比較に有利)
- fabrication.zip は gerber + drill を合成したもの。2つのソースディレクトリからファイルを集める

## 採用/不採用判断

**採用**: `zip` v2 + `walkdir` によるディレクトリ再帰 ZIP 作成

## 制約とpitfall

1. **メモリ使用量**: `read_to_end` は小さなファイル向き。大ファイルには `std::io::copy` を使う
2. **パスセパレータ**: ZIP 仕様はスラッシュ (`/`) のみ。Windows パスの `\` を変換する必要あり (Linux では不要だが防御的に)
3. **空ディレクトリ**: `add_directory` で明示的に追加しないと ZIP に含まれない。BoardFlow の spec ではディレクトリ構造が決まっているので、空でも追加すべき
4. **大きな ZIP**: BufWriter でラップすると seek が flush を呼ぶ問題がある。ZipWriter は内部でバッファリングするので BufWriter は不要
5. **エラーハンドリング**: `zip.finish()` を呼ばないと ZIP が壊れる。`?` で early return する場合も finish を忘れないこと

## 未解決の疑問

- なし

## 参照URL

- https://docs.rs/zip/2/zip/
- https://crates.io/crates/zip
- https://docs.rs/walkdir/2/walkdir/
- https://github.com/zip-rs/zip2/tree/master/examples
