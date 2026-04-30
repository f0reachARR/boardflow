# SQLx Migration ファイル形式 (0.8)

対象Issue: #2

## 要約

SQLx 0.8 のマイグレーションは `migrations/` ディレクトリ内に `<timestamp>_<name>.sql` 形式のファイルを配置する。タイムスタンプは14桁 `YYYYMMDDHHmmss`。非可逆（simple）と可逆（reversible）の2形式がある。

## 確認した情報

### ファイル命名規則

| 形式 | ファイル名パターン | CLI コマンド |
|---|---|---|
| Simple (非可逆) | `<timestamp>_<name>.sql` | `sqlx migrate add <name>` |
| Reversible (可逆) | `<timestamp>_<name>.up.sql` + `<timestamp>_<name>.down.sql` | `sqlx migrate add -r <name>` |

- タイムスタンプ: 14桁 `YYYYMMDDHHmmss` (例: `20260430000000`)
- 実行順序: ファイル名のレキシコグラフィック順（タイムスタンプが先頭なので自動的に時系列順）
- ディレクトリ: crate ルートからの相対パス `migrations/`（カスタマイズ可能）

### 実行コマンド

```bash
# マイグレーション実行
sqlx migrate run

# マイグレーション作成
sqlx migrate add <name>         # simple
sqlx migrate add -r <name>     # reversible

# revert (reversible のみ)
sqlx migrate revert
```

### 管理テーブル

SQLx は `_sqlx_migrations` テーブルを自動作成し、適用済みマイグレーションを記録する。

### Simple vs Reversible の選択基準

- **Simple**: ロールバック不要なプロジェクト初期、forward-only migration 方針
- **Reversible**: 開発中のスキーマ反復が多い場合、ブランチ間の切り替えが頻繁な場合

### 注意事項

- Simple と Reversible を同一プロジェクトで混在させることは可能（SQLx 0.8 で対応済み）
- 一度適用されたマイグレーションファイルの内容を変更してはならない（チェックサムで検証される）
- `sqlx migrate run` はプログラムからも `sqlx::migrate!()` マクロで実行可能

## BoardFlow への示唆

- 既存の `20260430000000_init.sql` は simple 形式を採用済み
- Issue #2 で追加する13テーブルのマイグレーションも simple 形式で統一する
- ファイル名例: `20260430000001_create_schema.sql`
- MVP段階では forward-only で十分。down migration は運用開始後に必要に応じて追加

## 採用/不採用判断

**採用**: Simple (非可逆) 形式

理由:
- 既存マイグレーションが simple 形式
- MVP 段階ではロールバックより再作成のほうが現実的
- 13テーブル全体を1ファイルに入れるか分割するかは実装者判断（推奨: 1ファイルで全テーブル作成）

## 制約とpitfall

- マイグレーションファイルのチェックサムがDBに記録されるため、適用済みファイルの変更は不可
- `sqlx migrate run` は冪等ではない（適用済みは skip するが、途中で失敗した場合手動対応が必要）
- PostgreSQL ではマイグレーション全体が1トランザクションで実行される（DDLがトランザクショナル）

## 未解決の疑問

なし

## 参照URL

- https://docs.rs/sqlx/latest/sqlx/migrate/index.html
- https://github.com/launchbadge/sqlx/issues/881 (命名規則の議論)
- https://github.com/launchbadge/sqlx/issues/1306 (down migration ドキュメント)
- https://crates.io/crates/sqlx-cli
