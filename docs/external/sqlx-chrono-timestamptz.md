# chrono::DateTime<Utc> と SQLx TIMESTAMPTZ マッピング

対象Issue: #2

## 要約

SQLx 0.8 の `chrono` feature を有効にすると、PostgreSQL の `TIMESTAMPTZ` カラムと Rust の `chrono::DateTime<Utc>` が直接マッピングされる。追加の実装は不要。DDL では `DEFAULT now()` を使い、Rust 側では `DateTime<Utc>` 型でフィールドを定義する。

## 確認した情報

### 型マッピング

| PostgreSQL 型 | Rust 型 | SQLx feature |
|---|---|---|
| `TIMESTAMPTZ` | `chrono::DateTime<Utc>` | `chrono` |
| `TIMESTAMPTZ` | `chrono::DateTime<FixedOffset>` | `chrono` |
| `TIMESTAMPTZ` | `chrono::DateTime<Local>` | `chrono` |
| `TIMESTAMP` | `chrono::NaiveDateTime` | `chrono` |
| `DATE` | `chrono::NaiveDate` | `chrono` |
| `TIME` | `chrono::NaiveTime` | `chrono` |

### Encode / Decode 実装

```rust
// SQLx 0.8 で自動的に利用可能
impl<Tz> Encode<'_, Postgres> for DateTime<Tz> where Tz: TimeZone
impl<'r> Decode<'r, Postgres> for DateTime<Utc>
impl<'r> Decode<'r, Postgres> for DateTime<FixedOffset>
impl<'r> Decode<'r, Postgres> for DateTime<Local>
```

- `Encode`: 任意の `TimeZone` を持つ `DateTime` を PostgreSQL に送信可能（内部で UTC に変換）
- `Decode`: `DateTime<Utc>` として取り出すと、DB に保存された値が UTC として解釈される

### DDL パターン

```sql
CREATE TABLE board_runs (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);
```

- `DEFAULT now()`: PostgreSQL のトランザクション開始時刻（`CURRENT_TIMESTAMP` と同等）
- `TIMESTAMPTZ`: 内部的には UTC で保存、表示時にセッションタイムゾーンで変換

### Rust 構造体パターン

```rust
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub struct BoardRun {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,  // nullable
}
```

### Cargo.toml（既に設定済み）

```toml
[workspace.dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "migrate"] }
```

### 注意: TIMESTAMP vs TIMESTAMPTZ

- `TIMESTAMP` (without time zone): タイムゾーン情報なし → `NaiveDateTime` にマッピング
- `TIMESTAMPTZ` (with time zone): UTC で保存 → `DateTime<Utc>` にマッピング
- BoardFlow では全タイムスタンプを `TIMESTAMPTZ` で統一すべき

## BoardFlow への示唆

- 全 `created_at`、`updated_at`、`completed_at` 等は `TIMESTAMPTZ NOT NULL DEFAULT now()`
- nullable なタイムスタンプ（`completed_at`、`revoked_at` 等）は `TIMESTAMPTZ` (DEFAULT なし)
- Rust 側: `DateTime<Utc>` (必須) / `Option<DateTime<Utc>>` (nullable)
- `now()` はアプリ層ではなく DB 側で生成（トランザクション内の一貫性のため）

## 採用/不採用判断

**採用**: `TIMESTAMPTZ` + `DEFAULT now()` + `DateTime<Utc>`

理由:
- SQLx の chrono feature で追加実装なしにマッピング可能
- TIMESTAMPTZ はタイムゾーン情報を保持し、UTC 正規化される
- `DEFAULT now()` により INSERT 時にアプリ側で時刻を渡さなくてよい
- Cargo.toml に必要な feature が既に設定済み

## 制約とpitfall

- `TIMESTAMPTZ` は内部的に UTC で保存するため、`SET timezone` によらず正しい値が返る
- `now()` はトランザクション開始時刻 → 長いトランザクション内では同一値になる（通常は望ましい動作）
- chrono の `DateTime<Utc>` はマイクロ秒精度。PostgreSQL のマイクロ秒精度と一致するため問題なし
- `Option<DateTime<Utc>>` を使う場合、クエリで `NULL` が返る可能性のあるカラムに必ず `Option` を付けること

## 未解決の疑問

なし

## 参照URL

- https://docs.rs/sqlx/latest/sqlx/types/chrono/struct.DateTime.html
- https://docs.rs/sqlx/latest/sqlx/postgres/types/index.html
- https://www.postgresql.org/docs/16/datatype-datetime.html
