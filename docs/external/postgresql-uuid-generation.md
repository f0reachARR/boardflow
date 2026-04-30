# PostgreSQL UUID 生成方法

対象Issue: #2

## 要約

PostgreSQL 16 では `gen_random_uuid()` がビルトイン（拡張不要）で UUID v4 を生成できる。ただし UUID v7 はPostgreSQL 18+ の `uuidv7()` が必要であり、PG 16 ではアプリケーション層で生成する必要がある。BoardFlow は UUID v7 を Rust 側で生成し、DB には UUID 型カラムとして保存する方針を推奨する。

## 確認した情報

### gen_random_uuid() の利用可能性

| PostgreSQL バージョン | gen_random_uuid() | 拡張要否 | UUID バージョン |
|---|---|---|---|
| 9.x – 12.x | pgcrypto 拡張が必要 | `CREATE EXTENSION pgcrypto` | v4 |
| **13+** | **ビルトイン** | **不要** | v4 |
| 18+ | ビルトイン + `uuidv7()` 追加 | 不要 | v4 / v7 |

### UUID v7 について

- RFC 9562 で定義（2024年、RFC 4122 を置き換え）
- タイムスタンプベース + ランダム → B-tree インデックスに有利（時系列ソート可能）
- PostgreSQL ネイティブの `uuidv7()` 関数は **PostgreSQL 18+** のみ（2025年リリース）
- PostgreSQL 16 では DB 側で v7 を生成する組み込み手段がない

### Rust uuid crate での UUID v7 生成

```rust
use uuid::Uuid;

// UUID v7 生成（timestamp-based + random）
let id = Uuid::now_v7();
```

- `uuid` crate の `v7` feature で利用可能（Cargo.toml に既に設定済み）
- SQLx の `uuid` feature で `Uuid` 型が PostgreSQL の `UUID` 型にマッピングされる

### DDL での DEFAULT 値

```sql
-- v4 を DEFAULT にする場合（PG 13+）
id UUID PRIMARY KEY DEFAULT gen_random_uuid()

-- v7 をアプリ層で生成する場合、DEFAULT なしでも可
id UUID PRIMARY KEY
```

## BoardFlow への示唆

- **UUID v7 をアプリケーション層（Rust）で生成**し、INSERT 時に渡す
- DDL には `DEFAULT gen_random_uuid()` を設定しない（v4 が混在するため）
- ただし、緊急時のデバッグ用に `DEFAULT gen_random_uuid()` を残す選択肢もある（v4 が入るが UUID 型として valid）
- 推奨 DDL: `id UUID PRIMARY KEY` （DEFAULT なし、Rust 側で `Uuid::now_v7()` 生成）

## 採用/不採用判断

**採用**: アプリ層 UUID v7 生成（`Uuid::now_v7()`）

理由:
- PG 16 には `uuidv7()` がない
- UUID v7 は時系列ソート可能で B-tree インデックス性能が v4 より優れる
- Cargo.toml で `uuid = { features = ["v7"] }` が既に設定済み
- `gen_random_uuid()` は拡張不要だが v4 のみ

## 制約とpitfall

- アプリ層生成のため、直接 SQL で INSERT する場合（デバッグ・データ修正）は `gen_random_uuid()` (v4) になる
- UUID v7 のタイムスタンプ部はミリ秒精度 → 同一ミリ秒内の順序はランダム部に依存
- `Uuid::now_v7()` はシステム時刻に依存するため、時刻がずれている環境では順序保証が崩れる

## 未解決の疑問

なし

## 参照URL

- https://www.postgresql.org/docs/16/functions-uuid.html
- https://docs.rs/uuid/latest/uuid/struct.Uuid.html#method.now_v7
- https://www.ietf.org/rfc/rfc9562 (UUID v7 仕様)
- https://maciejwalkowiak.com/blog/postgres-uuid-primary-key/ (UUID v7 性能比較)
