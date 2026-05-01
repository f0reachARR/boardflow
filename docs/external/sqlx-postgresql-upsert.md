# SQLx PostgreSQL Upsert (ON CONFLICT) パターン

## 要約

SQLx 0.8 で PostgreSQL の `INSERT ... ON CONFLICT ... DO UPDATE ... RETURNING` を使ったupsert（insert or update）を実装する推奨パターンをまとめる。`query_as` との組み合わせ方、bind パラメータの扱い、BoardFlow の Plan API で必要な Repository / BoardProject の upsert に焦点を当てる。

## 確認した情報

### 基本パターン: `sqlx::query_as` + upsert + RETURNING

SQLx では SQL 文字列リテラルを直接記述する。PostgreSQL の `ON CONFLICT` 構文をそのまま使える。`RETURNING` で upsert 後の行を返し、`query_as` で Rust 構造体にマッピングする。

```rust
use sqlx::PgPool;
use uuid::Uuid;

pub async fn upsert_repository(
    pool: &PgPool,
    id: Uuid,
    github_repository_id: i64,
    owner: &str,
    name: &str,
    installation_id: i64,
) -> Result<Repository, sqlx::Error> {
    sqlx::query_as::<_, Repository>(
        r#"
        INSERT INTO repositories (id, github_repository_id, owner, name, installation_id, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
        ON CONFLICT (github_repository_id)
        DO UPDATE SET
            owner = EXCLUDED.owner,
            name = EXCLUDED.name,
            installation_id = EXCLUDED.installation_id,
            updated_at = NOW()
        RETURNING id, github_repository_id, owner, name, installation_id, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(github_repository_id)
    .bind(owner)
    .bind(name)
    .bind(installation_id)
    .fetch_one(pool)
    .await
}
```

### 複合 UNIQUE 制約での ON CONFLICT

BoardProject は `(repository_id, project_path)` の複合 UNIQUE 制約を持つ。ON CONFLICT の対象に複数カラムを指定する。

```rust
pub async fn upsert_board_project(
    pool: &PgPool,
    id: Uuid,
    repository_id: Uuid,
    project_path: &str,
    project_dir: &str,
    display_name: &str,
) -> Result<BoardProject, sqlx::Error> {
    sqlx::query_as::<_, BoardProject>(
        r#"
        INSERT INTO board_projects (
            id, repository_id, project_path, project_dir, display_name,
            issue_sync_status, recreate_issue_on_update, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, 'pending', true, NOW(), NOW())
        ON CONFLICT (repository_id, project_path)
        DO UPDATE SET
            project_dir = EXCLUDED.project_dir,
            display_name = EXCLUDED.display_name,
            updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(repository_id)
    .bind(project_path)
    .bind(project_dir)
    .bind(display_name)
    .fetch_one(pool)
    .await
}
```

### EXCLUDED 仮想テーブル

`DO UPDATE SET` 内で `EXCLUDED.column_name` を使うと、INSERT しようとした値（conflict した新しい値）を参照できる。既存の行の値は通常のカラム名で参照する。

```sql
DO UPDATE SET
    owner = EXCLUDED.owner,          -- 新しい値で更新
    name = EXCLUDED.name,            -- 新しい値で更新
    updated_at = NOW()               -- 定数値で更新
```

### RETURNING で全カラムを返す

`RETURNING *` で全カラムを返せるが、`sqlx::FromRow` derive の構造体のフィールド順とカラム順が一致している必要がある（SQLx は名前ベースでマッチングするので順序は実際には問わない）。明示的に `RETURNING col1, col2, ...` と列挙する方が安全。

### query_as vs query! (compile-time checked)

- `query_as::<_, T>(sql)`: ランタイムのSQL実行。`T: FromRow` を要求。動的SQLに向く。
- `query!(sql)` / `query_as!(T, sql)`: コンパイル時にDBスキーマとSQL文を検証。型安全。ただしDBへの接続が必要。

BoardFlow では現在 `query_as` パターンを使用しており（既存の `api_token` クエリで確認）、一貫性のため同じパターンを踏襲する。将来 `query_as!` への移行も可能。

### トランザクション内での upsert

Plan API では Repository と複数 BoardProject を一つのトランザクションで upsert する必要がある。

```rust
let mut tx = pool.begin().await?;

let repo = sqlx::query_as::<_, Repository>(/* upsert SQL */)
    .bind(/* ... */)
    .fetch_one(&mut *tx)
    .await?;

for project in &req.projects {
    let bp = sqlx::query_as::<_, BoardProject>(/* upsert SQL */)
        .bind(/* ... */)
        .fetch_one(&mut *tx)
        .await?;
    // snapshot comparison, decision logic...
}

tx.commit().await?;
```

`&mut *tx` で Transaction からの executor reference を渡す。

## BoardFlow への示唆

Plan API の実装で必要な upsert は以下の2つ:

1. **Repository upsert**: `ON CONFLICT (github_repository_id) DO UPDATE` — owner, name, installation_id を更新
2. **BoardProject upsert**: `ON CONFLICT (repository_id, project_path) DO UPDATE` — project_dir, display_name を更新

いずれも `RETURNING` で upsert 後の完全な行を取得し、後続の差分判定ロジックに使う。一つのトランザクション内で実行する。

## 採用/不採用判断

**採用**: `query_as` + `INSERT ... ON CONFLICT ... DO UPDATE ... RETURNING` パターンを使用。既存の SQLx 利用パターンと一貫性があり、PostgreSQL の標準機能で実現可能。

## 制約と pitfall

- `ON CONFLICT` の対象カラム/制約は、テーブル定義の UNIQUE 制約と一致する必要がある
- `DO UPDATE SET` で更新しないカラム（例: `created_at`）は `EXCLUDED` 参照しないこと（意図せず上書きされる）
- `RETURNING *` は便利だが、将来のスキーマ変更でカラムが増えた場合に `FromRow` derive の構造体と不整合になる可能性がある
- `NOW()` は同一トランザクション内で一貫した値を返す（PostgreSQL の仕様）
- 複合 UNIQUE 制約のカラム順はSQL上は問わないが、明示的に記述する方が可読性が高い
- Upsert 時に生成する UUID (id) は、conflict 時に EXCLUDED.id が使われないため無駄になるが問題ない

## 未解決の疑問

なし。SQLx + PostgreSQL の upsert パターンは安定しており、既存コードとの一貫性も問題ない。

## 参照URL

- https://www.postgresql.org/docs/current/sql-insert.html#SQL-ON-CONFLICT （PostgreSQL 公式: INSERT ON CONFLICT）
- https://docs.rs/sqlx/0.8/sqlx/fn.query_as.html （SQLx query_as ドキュメント）
- https://www.prisma.io/dataguide/postgresql/inserting-and-modifying-data/insert-on-conflict （PostgreSQL upsert 解説ガイド）
