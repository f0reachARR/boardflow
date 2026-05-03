# Issue #62: OpenAPI定義からschema.d.ts生成の運用整備

## Issueまでの経緯

- boardflow/package.jsonにgenerate:apiスクリプトが既に定義済み（ポート3001指定）
- openapi-typescript v7とopenapi-fetchは依存に含まれ、schema.d.tsも存在する
- Issue #29 (Frontend: Next.jsプロジェクトセットアップ) で基盤構築済み（CLOSED）
- 実際のAPIサーバデフォルトポートは3000（api/config.rsのapi_port）
- schema.d.tsが最新のAPI定義と同期されているか不明確
- 開発時のワークフローとして型生成手順が明文化されていない

## ユーザー要望

APIサーバ側のOpenAPI定義が適切に活用されていない。APIサーバを起動してOpenAPI定義を取得し、schema.d.tsを生成する必要がある。.envは用意済み。

## Issue作成内容

- Issue #62として新規作成
- labels: frontend, infrastructure
- ポート番号修正、schema.d.ts再生成、手順ドキュメント化

## 後続処理タイプの初期仮説

`implementation_required`

## 調査フェーズ（2026-05-03）

### 調査対象

1. openapi-typescript v7 CLIオプションの変更有無
2. v6→v7 の破壊的変更がBoardFlowに影響するか

### 調査結果

#### openapi-typescript v7 CLIオプション

- **基本構文に変更なし**: `openapi-typescript <URL> -o <output>` は v7 でもそのまま動作
- v7 の破壊的変更は以下の4点だが、いずれもBoardFlowの利用パターン（単一リモートURL + `-o`）には影響しない:
  1. 認証がRedocly CLI設定に移行 → ローカルAPIサーバなので認証不要
  2. TypeScript AST導入 → CLI利用のみなので無関係
  3. `defaultNonNullable: true` がデフォルト → 生成結果に軽微な差分の可能性
  4. Globbing廃止→Redocly config → 単一スキーマなので無関係
- `--check` フラグが追加（生成済み型の最新チェック、CI連携に有用）

#### 実装に必要な作業（確認済み）

1. `boardflow/package.json` の `generate:api` スクリプト: ポート `3001` → `3000` に修正
2. APIサーバを起動して `pnpm generate:api` を実行し `schema.d.ts` を再生成
3. 開発手順のドキュメント化（README or docs/frontend/summary.md）

### 参照URL

- https://openapi-ts.dev/cli - 公式CLIドキュメント
- https://openapi-ts.dev/migration-guide - v6→v7マイグレーションガイド

### 結論ステータス

`implementation_required`

### 後続エージェントへの注意点

- ポート修正は `boardflow/package.json` の `generate:api` スクリプト内の `3001` → `3000` のみ
- `boardflow/src/lib/api/server.ts` の `API_BASE_URL` デフォルト値 `http://localhost:3001` も `3000` に統一すべきか検討（ただしIssue #62スコープ外の可能性）
- APIサーバ起動コマンド: `mise exec -- cargo run -p boardflow-api`
- `schema.d.ts` は現在手動作成のコメント付き。再生成後は自動生成コメントに置き換わる
- `--check` フラグのCI連携は将来的な改善として記録

## 残リスク

- APIサーバ起動が必要な型生成フローはCI上での自動化が複雑
- ポート番号はenvで変更可能なため、固定値の是非
- `defaultNonNullable: true` のデフォルト変更により、既存の手動schema.d.tsと自動生成の差分が大きくなる可能性（ただし手動→自動への移行なので問題なし）

---

## 計画フェーズ（2026-05-03）

### 目的

- APIサーバのOpenAPI定義（`/api/v1/openapi.json`）から `schema.d.ts` を自動生成し、手動スタブを置き換える
- ポート番号をAPIデフォルト（3000）に統一し、開発時の混乱を排除する
- 開発者がschema.d.tsを再生成する手順をドキュメント化する

### 非目的

- CI/CDパイプラインでの自動型生成（将来Issue）
- openapi-typescript `--check` フラグによるCI連携（将来Issue）
- Next.js開発サーバのポート設定変更（開発者がPORT env等で制御する前提）
- APIのOpenAPIスキーマ自体の変更

### 受け入れ条件

1. `pnpm generate:api` がAPIサーバ（port 3000）からスキーマを取得して `schema.d.ts` を生成できる
2. `server.ts` と `next.config.ts` のAPIデフォルトURLがポート3000を使用する
3. `pnpm typecheck` がエラーなく通る
4. README.mdの開発手順が実態と整合する
5. `schema.d.ts` が自動生成された内容に置き換わる

### 詳細要件

| # | 要件 | 対象ファイル |
|---|------|-------------|
| 1 | `generate:api` のURLポートを3001→3000に修正 | `boardflow/package.json` |
| 2 | `API_BASE_URL` デフォルト値を3001→3000に修正 | `boardflow/src/lib/api/server.ts` |
| 3 | rewrites destination デフォルト値を3001→3000に修正 | `boardflow/next.config.ts` |
| 4 | `.env.local.example` のポートを3000に修正 | `boardflow/.env.local.example` |
| 5 | APIサーバ起動→schema.d.ts自動生成で手動スタブを置換 | `boardflow/src/lib/api/schema.d.ts` |
| 6 | READMEの「バックエンドが3001で起動」記述を修正 | `README.md` |
| 7 | 型生成手順の詳細ドキュメント追記 | `README.md` |

### 影響範囲

- **フロントエンドAPI型定義**: 自動生成のschema.d.tsに切り替わるため、既存コードで参照している型がOpenAPIスキーマと整合しない場合はコンパイルエラーになる可能性あり
- **開発環境設定**: ポート番号デフォルト変更により、既存の `.env.local` を使っている開発者は影響なし（env変数優先のため）。新規セットアップ時のみ影響
- **`client.ts`**: `baseUrl: ""` で相対パス使用のため変更不要

### 設計方針

- ポート番号のデフォルト値はすべて3000に統一（APIサーバのデフォルトに合わせる）
- Next.js開発サーバとAPIサーバのポート衝突は、Next.js側を別ポートで起動する運用（`PORT=3001 pnpm dev` or Next.jsのauto-port-detection）で対応
- `schema.d.ts` は完全に自動生成物とし、手動編集しない方針を明記

### 作業ブランチ

`feature/issue-62-openapi-schema-generation`

### 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `boardflow/package.json` | `generate:api` のポート3001→3000 |
| `boardflow/src/lib/api/server.ts` | `API_BASE_URL` デフォルト3001→3000 |
| `boardflow/next.config.ts` | rewrites destination デフォルト3001→3000 |
| `boardflow/.env.local.example` | `API_BASE_URL=http://localhost:3000` |
| `boardflow/src/lib/api/schema.d.ts` | 自動生成で完全置換 |
| `README.md` | ポート記述修正 + 型生成手順追記 |

### 実装手順

1. **ブランチ作成**: `git checkout -b feature/issue-62-openapi-schema-generation`
2. **ポート修正（4ファイル）**:
   - `boardflow/package.json`: `generate:api` の `3001` → `3000`
   - `boardflow/src/lib/api/server.ts`: デフォルト値 `3001` → `3000`
   - `boardflow/next.config.ts`: rewrites destination `3001` → `3000`
   - `boardflow/.env.local.example`: `3001` → `3000`
3. **APIサーバ起動**: `mise exec -- cargo run -p boardflow-api`
4. **schema.d.ts生成**: `cd boardflow && pnpm generate:api`
5. **型チェック**: `pnpm typecheck`
   - エラーがあれば、自動生成されたスキーマと既存コードの不整合を修正
6. **README更新**: ポート記述と型生成手順を修正・追記
7. **最終確認**: `pnpm lint && pnpm typecheck`
8. **コミット＆プッシュ**

### テスト観点

| テスト | 方法 | 期待結果 |
|--------|------|---------|
| 型生成 | `pnpm generate:api`（APIサーバ起動中） | schema.d.ts が生成される |
| 型チェック | `pnpm typecheck` | エラーなし |
| Lint | `pnpm lint` | エラーなし |
| ビルド | `pnpm build` | 成功（Optional、型チェックで十分） |

### ドキュメント更新対象

- `README.md`: ポート記述修正、型生成手順セクション追記
- `boardflow/.env.local.example`: ポート修正

### 注意事項

1. **APIサーバ起動必須**: `pnpm generate:api` 実行時はAPIサーバが起動している必要がある。DBやRedis等の依存も必要（`docker-compose up` 前提）
2. **型の差分**: 手動スタブ→自動生成への移行により、型名やプロパティ名が変わる可能性がある。既存のフロントエンドコードが型エラーになった場合は修正が必要
3. **ポート衝突**: Next.js(`pnpm dev`)とAPIサーバが同時起動する場合、Next.jsは別ポートで起動する必要がある（`PORT=3001 pnpm dev` 等）

### 実装要否

`implementation_required`

### 未解決の疑問

- なし（ユーザーからの情報で全て解決済み）
