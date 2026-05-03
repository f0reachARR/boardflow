# openapi-typescript v7 CLI オプション調査

## 要約

openapi-typescript v7 のCLIは、単一リモートスキーマ + `-o` オプションの基本的な使い方に変更なし。BoardFlow の `generate:api` スクリプトはポート番号修正のみで動作する。v7 では `--check` フラグが追加されており、CI での型同期チェックに利用可能。

## 確認した情報

### v7 CLIの基本構文（変更なし）

```bash
# リモートスキーマから生成（BoardFlowで使用するパターン）
npx openapi-typescript http://localhost:3000/api/v1/openapi.json -o ./src/lib/api/schema.d.ts
```

- `-o` / `--output`: 出力先ファイルパス指定（変更なし）
- リモートURL指定: そのまま動作（変更なし）
- JSON / YAML 両対応（変更なし）

### v7 での破壊的変更（6.x → 7.x）

| 変更点 | 影響範囲 | BoardFlowへの影響 |
|---|---|---|
| 認証がRedocly CLI設定に移行 | リモートスキーマの認証 | **なし**（ローカルAPIサーバなので認証不要） |
| TypeScript AST導入 | Node.js API（プログラマティック利用） | **なし**（CLI利用のみ） |
| `defaultNonNullable: true` がデフォルト | default値を持つスキーマの型生成 | **軽微**（生成結果が若干変わる可能性） |
| Globbing廃止→Redocly config | 複数スキーマの一括生成 | **なし**（単一スキーマのみ） |
| Node.js API入力型変更 | プログラマティック利用 | **なし**（CLI利用のみ） |

### v7 で追加された有用なフラグ

| フラグ | 説明 | 活用可能性 |
|---|---|---|
| `--check` | 生成済み型が最新か検証（差分あればexit code 1） | CI連携に有用 |
| `--export-type` / `-t` | interfaceの代わりにtype exportを生成 | 好みに応じて |
| `--enum` | string unionの代わりにTS enumを生成 | 必要に応じて |
| `--immutable` | readonly型を生成 | レスポンス型の安全性向上 |
| `--exclude-deprecated` | 非推奨フィールドを除外 | スキーマが安定したら検討 |
| `--alphabetize` | プロパティをアルファベット順にソート | diff安定性向上 |

### 推奨コマンド

```bash
# 最小限（現行と同等）
openapi-typescript http://localhost:3000/api/v1/openapi.json -o ./src/lib/api/schema.d.ts

# CI用チェック
openapi-typescript http://localhost:3000/api/v1/openapi.json -o ./src/lib/api/schema.d.ts --check
```

## BoardFlow への示唆

- `generate:api` スクリプトのポートを `3001` → `3000` に修正するだけで動作する
- `--check` フラグをCIに組み込めば、型定義の同期漏れを検知できる（ただしAPIサーバ起動が前提）
- 現状のCLI利用パターン（単一リモートURL + `-o`）はv7でも完全にサポートされている

## 採用/不採用判断

- **採用**: 現行の `openapi-typescript` CLI 利用を継続。ポート修正のみで対応可能
- `redocly.yaml` 設定は不要（単一スキーマのため）
- 追加フラグ（`--check` 等）は実装フェーズで検討

## 制約とpitfall

- APIサーバが起動していないと `generate:api` が失敗する（リモートURL取得のため）
- `defaultNonNullable: true` がv7デフォルトのため、v6で生成した型と若干差分が出る可能性がある
- Redocly CLIがバリデーションに使われるため、スキーマに問題があるとエラーになることがある

## 未解決の疑問

- なし（CLIオプションに関する調査は完了）

## 参照URL

- https://openapi-ts.dev/cli - 公式CLIドキュメント
- https://openapi-ts.dev/migration-guide - v6→v7マイグレーションガイド
- https://github.com/openapi-ts/openapi-typescript - GitHubリポジトリ
