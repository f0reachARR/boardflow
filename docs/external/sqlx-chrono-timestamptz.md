# chrono::DateTime<Utc> と SQLx TIMESTAMPTZ マッピング

対象Issue: #2

## 要約

SQLx 0.8 の `chrono` feature を有効にすると、PostgreSQL の `TIMESTAMPTZ` カラムと Rust の `chrono::DateTime<Utc>` が直接マッピングされる。追加の実装は不要。BoardFlow ではタイムスタンプをアプリ層（`chrono::Utc::now()`）で生成し、DDL に `DEFAULT now()` は付けない。

## SQLx の一般的な型マッピング

### 型対応表

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

### TIMESTAMP vs TIMESTAMPTZ

- `TIMESTAMP` (without time zone): タイムゾーン情報なし → `NaiveDateTime` にマッピング
- `TIMESTAMPTZ` (with time zone): UTC で保存 → `DateTime<Utc>` にマッピング

### Cargo.toml 設定

```toml
[workspace.dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "uuid", "chrono", "migrate"] }
```

## BoardFlow の採用判断

**採用**: `TIMESTAMPTZ` + アプリ層で timestamp 設定（DDL に DEFAULT なし）+ `DateTime<Utc>`

### 方針

- 全タイムスタンプは DDL 上 `TIMESTAMPTZ NOT NULL`（DEFAULT なし）
- `created_at`、`updated_at` の値は Rust 側で `chrono::Utc::now()` を設定してから INSERT/UPDATE する
- nullable なタイムスタンプ（`completed_at`、`revoked_at` 等）は `TIMESTAMPTZ`（DEFAULT なし）
- Rust 側: `DateTime<Utc>` (必須) / `Option<DateTime<Utc>>` (nullable)

### 理由（Issue #2 実装時に決定）

- テスト時に特定のタイムスタンプを注入でき、再現性が高い
- アプリ層で一貫して `Utc::now()` を呼ぶことで、INSERT 文の責務が明確になる
- DB 側 DEFAULT に依存しないため、マイグレーション間の暗黙的な挙動差が生じない
- SQLx の chrono feature で追加実装なしにマッピング可能
- TIMESTAMPTZ はタイムゾーン情報を保持し、UTC 正規化される

### DDL パターン

```sql
CREATE TABLE board_runs (
    id UUID PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ
);
```

- `TIMESTAMPTZ`: 内部的には UTC で保存、表示時にセッションタイムゾーンで変換
- DEFAULT は付けない。INSERT/UPDATE 時にアプリ層から値を渡す

### Rust 構造体パターン

```rust
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub struct BoardRun {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,  // nullable
}
```

### INSERT 時のパターン

```rust
let now = chrono::Utc::now();
sqlx::query!(
    r#"INSERT INTO board_runs (id, created_at, updated_at) VALUES ($1, $2, $3)"#,
    id,
    now,
    now,
)
.execute(&pool)
.await?;
```

## 注意事項

- `TIMESTAMPTZ` は内部的に UTC で保存するため、`SET timezone` によらず正しい値が返る
- PostgreSQL の `now()` はトランザクション開始時刻を返す。BoardFlow ではアプリ層で `Utc::now()` を呼ぶため、同一トランザクション内の複数 INSERT で微妙に異なる時刻が入り得る点に留意
- chrono の `DateTime<Utc>` はマイクロ秒精度。PostgreSQL のマイクロ秒精度と一致するため問題なし
- `Option<DateTime<Utc>>` を使う場合、クエリで `NULL` が返る可能性のあるカラムに必ず `Option` を付けること

## 参照URL

- https://docs.rs/sqlx/latest/sqlx/types/chrono/struct.DateTime.html
- https://docs.rs/sqlx/latest/sqlx/postgres/types/index.html
- https://www.postgresql.org/docs/16/datatype-datetime.html
