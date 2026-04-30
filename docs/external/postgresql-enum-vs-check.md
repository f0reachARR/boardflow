# PostgreSQL ENUM vs CHECK 制約

対象Issue: #2

## 要約

BoardFlow のステータスカラム（`board_runs.status`、`artifacts.status` 等）には **TEXT + CHECK 制約** を採用する。マイグレーションでの値追加・削除が容易で、SQLx との統合もシンプルであるため。

## 確認した情報

### Native ENUM の特徴

```sql
CREATE TYPE run_status AS ENUM ('created', 'uploading', 'importing', 'completed', 'failed', 'timed_out');
```

**利点:**
- ストレージ効率が良い（4バイトの内部参照）
- 型安全性（異なる ENUM 間の比較がコンパイルエラー）
- 宣言順による自動的な順序付け

**欠点:**
- 値の削除が極めて困難（型の再作成 + ACCESS EXCLUSIVE ロック）
- `ALTER TYPE ... ADD VALUE` はトランザクション内で実行可（PG 12+）だが、値の削除・名前変更は依然困難
- マイグレーションツールとの相性が悪い（DDL変更が複雑）

### TEXT + CHECK 制約の特徴

```sql
CREATE TABLE board_runs (
    status TEXT NOT NULL CHECK (status IN ('created', 'uploading', 'importing', 'completed', 'failed', 'timed_out'))
);
```

**利点:**
- 値の追加・削除がシンプル（制約の DROP + 再作成、O(1) + VALIDATE）
- `NOT VALID` で新規行のみ即座に制約適用、既存データは非同期検証
- ロックが軽い（SHARE UPDATE EXCLUSIVE、読み書き可能）
- SQLx での Rust マッピングが容易

**欠点:**
- ストレージ効率が低い（値のテキスト全体を保存）
- 異なるカラム間の型的制約なし（文字列として比較可能）

### CHECK 制約の変更手順

```sql
-- 1. 既存制約の削除 (O(1))
ALTER TABLE board_runs DROP CONSTRAINT board_runs_status_check;

-- 2. 新しい制約の追加 (NOT VALID で O(1)、新規 INSERT/UPDATE に即適用)
ALTER TABLE board_runs
    ADD CONSTRAINT board_runs_status_check
        CHECK (status IN ('created', 'uploading', 'importing', 'completed', 'failed', 'timed_out', 'cancelled'))
        NOT VALID;

-- 3. 既存データの検証 (SHARE UPDATE EXCLUSIVE ロック、読み書き可能)
ALTER TABLE board_runs VALIDATE CONSTRAINT board_runs_status_check;
```

### SQLx での Rust マッピング

#### TEXT + CHECK 方式（推奨）

```rust
#[derive(Debug, Clone, sqlx::Type, serde::Serialize, serde::Deserialize)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Uploading,
    Importing,
    Completed,
    Failed,
    TimedOut,
}
```

#### Native ENUM 方式（不採用）

```rust
#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "run_status", rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    Uploading,
    // ...
}
```

### 比較表

| 観点 | Native ENUM | TEXT + CHECK |
|---|---|---|
| ストレージ | 4 bytes | 値の文字列長 |
| 値追加 | `ALTER TYPE ADD VALUE` (PG 12+ でトランザクション可) | 制約 DROP + 再作成 |
| 値削除 | 型再作成 + ACCESS EXCLUSIVE ロック | 制約 DROP + 再作成 (軽量) |
| SQLx 統合 | `#[sqlx(type_name = "enum_name")]` | `#[sqlx(type_name = "text")]` |
| マイグレーション | 複雑 | シンプル |
| offline mode | 型情報の prepare が必要 | TEXT なので追加設定不要 |

## BoardFlow への示唆

- spec.md に定義されている全ステータスカラム（`board_runs.status`、`artifacts.status`、`run_checks.status` 等）に TEXT + CHECK を適用
- Rust 側では `#[derive(sqlx::Type)]` + `#[sqlx(type_name = "text", rename_all = "snake_case")]` でマッピング
- CHECK 制約名は `<table>_<column>_check` の命名規則で統一
- 将来の値追加はマイグレーションファイルで制約を DROP + 再作成

## 採用/不採用判断

**採用**: TEXT + CHECK 制約

理由:
- マイグレーションでの変更が圧倒的に容易
- ロックの影響が小さい（SaaS は可用性重視）
- SQLx offline mode との相性が良い
- ステータス値は短い文字列（最大 "importing" = 9文字）でストレージ差は無視できる
- Close.com、Crunchy Data 等の実績あるプロジェクトが同じ結論

## 制約とpitfall

- CHECK 制約名を明示的に付けないと自動生成名になり、DROP 時に名前を調べる必要がある
- `rename_all = "snake_case"` を使う場合、Rust の `TimedOut` は DB 上で `timed_out` になる → CHECK 制約内の値と一致させること
- 大量の enum 値がある場合（数百）は native ENUM のストレージ効率が意味を持つが、BoardFlow のケースでは最大10値程度なので無関係

## 未解決の疑問

なし

## 参照URL

- https://making.close.com/posts/native-enums-or-check-constraints-in-postgresql/
- https://www.crunchydata.com/blog/enums-vs-check-constraints-in-postgres
- https://docs.rs/sqlx/latest/sqlx/postgres/types/index.html
- https://users.rust-lang.org/t/sqlx-postgres-how-to-insert-a-enum-value/53044
