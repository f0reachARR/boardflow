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

- <https://openapi-ts.dev/cli> - 公式CLIドキュメント
- <https://openapi-ts.dev/migration-guide> - v6→v7マイグレーションガイド

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

## レビューフェーズ（2026-05-03）

### レビュー結果

- `git diff main` と関連ドキュメント、research 成果物、README、生成済み `schema.d.ts` を確認した。
- `schema.d.ts` は openapi-typescript の自動生成ヘッダに置き換わっており、手動スタブのままではないことを確認した。
- `boardflow/package.json` の `generate:api` は `http://localhost:3000/api/v1/openapi.json` を参照するよう修正されている。
- `boardflow/src/lib/api/server.ts` と `boardflow/next.config.ts` のデフォルト URL は `3000` に更新されている。
- ただし、実装内に `http://localhost:3001` の既定値が 2 箇所残存していた。
  - `boardflow/src/lib/auth.ts`
  - `boardflow/src/app/api/viewer-sources/[boardRunId]/route.ts`
- 上記残存により、`API_BASE_URL` 未設定時の一部サーバーサイド通信が 3001 を参照し続け、ポート統一が完了していない。
- README は backend を `http://localhost:3000` 前提としつつ frontend 開発サーバーも `http://localhost:3000` と記載しており、同時起動手順として不正確だった。
- `schema-types.ts` の convenience alias 追加自体は妥当。`DiffSummary` は generated 型が `unknown` のため、runtime guard 前提の frontend 専用 shape として導入されている点も一貫している。

### テスト結果

- `cd boardflow && pnpm typecheck`: 成功
- `cd boardflow && pnpm lint`: 成功
- `curl http://localhost:3000/api/v1/openapi.json`: 実行環境では接続不可（HTTP code `000` / exit code 7）のため、レビュー中に `pnpm generate:api` の再実行までは未確認

### ドキュメント確認

- `README.md` の型生成手順セクションは追加済み。
- ただし frontend の起動ポート説明が backend の 3000 前提と衝突しており、受け入れ条件 4 を満たしたとは判断しない。
- research 成果物 `docs/external/openapi-typescript-fetch.md` には 3001 前提の記述が残るが、実装参照先ではないため参考情報として扱う。必要なら後続で整合更新を検討。

### PR/完了結果

- `pr_ready: false`
- 必須修正:
   1. `boardflow/src/lib/auth.ts` と `boardflow/src/app/api/viewer-sources/[boardRunId]/route.ts` のデフォルト API URL を 3000 に統一する。
   2. `README.md` の frontend 開発手順を実態に合わせて修正し、backend が 3000 を使う場合の frontend 起動ポート（例: `PORT=3001 pnpm dev` または auto-port）を明記する。

### 残リスク

- `pnpm generate:api` の実行性は、レビュー時点では API サーバー未起動のため再現確認できていない。
- OpenAPI の `summary` が依然 `unknown` であるため、frontend 側の runtime guard 運用は継続前提。

---

## 再レビュー（2026-05-03）

### 再レビュー結果

- 前回指摘していた 2 件は修正済みであることを確認した。
  - `boardflow/src/lib/auth.ts` のデフォルト API URL は `http://localhost:3000` に更新済み。
  - `boardflow/src/app/api/viewer-sources/[boardRunId]/route.ts` のデフォルト API URL は `http://localhost:3000` に更新済み。
- `git diff main` の全体確認により、Issue #62 の目的である OpenAPI 生成運用整備に沿って以下が反映されていることを確認した。
  - `boardflow/package.json` の `generate:api` は `http://localhost:3000/api/v1/openapi.json` を参照。
  - `boardflow/src/lib/api/server.ts` と `boardflow/next.config.ts` のデフォルト API URL は `3000` に統一済み。
  - `boardflow/.env.local.example` は `API_BASE_URL=http://localhost:3000` に更新済み。
  - `boardflow/src/lib/api/schema.d.ts` は openapi-typescript の自動生成ヘッダを持つ生成物へ置換済み。
- `boardflow/src/**` を対象に `localhost:3001` を再検索し、残存がないことを確認した。
- `README.md` は backend を `3000`、frontend を `pnpm dev --port 3001` で起動する手順に修正されており、前回指摘のポート衝突説明不備は解消している。

### テスト結果

- `cd boardflow && pnpm typecheck`: 成功
- `boardflow/src/**` 内の `localhost:3001` 検索: 該当なし
- `git diff --name-only main` / `git diff --stat main`: 差分範囲を確認し、Issue #62 に関連する変更として妥当と判断

### ドキュメント確認

- `README.md` の開発手順は今回の受け入れ条件と整合している。
- `docs/external/openapi-typescript-v7-cli.md` の調査結果と実装内容に齟齬はない。
- 参考資料や過去ログには `localhost:3001` の旧記述が残るが、実装・README・受け入れ条件の対象外であり、今回の PR ブロッカーではない。

### PR/完了結果

- `pr_ready: true`
- 重大な指摘事項なし。

### 残リスク

- 今回の再レビューでは API サーバーを起動して `pnpm generate:api` を再実行していないため、生成コマンド自体の実行確認は差分と生成物ヘッダ、および既存テスト結果に基づく判断である。

---

## PR作成フェーズ（2026-05-03）

### PR作成前チェック

- `pr_ready: true`（再レビューOK、指摘2件修正済み）
- `docs_ready: true`（README・ドキュメント整合確認済み）
- 未コミット変更: なし（`git status` でクリーン確認）
- ブランチ: `feature/issue-62-openapi-schema-generation`（コミット2件）
- テスト: `pnpm typecheck` / `pnpm lint` 成功

### PR作成結果

- **ブランチpush**: `git push origin feature/issue-62-openapi-schema-generation` — 成功
- **PR URL**: <https://github.com/f0reachARR/boardflow/pull/67>
- **タイトル**: `feat(#62): OpenAPI定義からschema.d.ts生成の運用整備`
- **Closes**: #62

### 残リスク

- APIサーバを起動した状態での `pnpm generate:api` 実行確認はレビュー時未実施（差分・生成物ヘッダ・テスト結果に基づく判断）
- OpenAPI の `summary` が `unknown` のため、frontend 側の runtime guard 運用は継続前提
- `--check` フラグによるCI連携は将来Issue

---

## ドキュメント再確認（2026-05-03）

### レビュー結果

- Issue #62 の前回指摘 2 件について、`README.md` の該当箇所を再確認した。
- Frontend ローカル開発手順には、バックエンド API 起動に `DATABASE_URL` 等の環境変数が必要であり、`docker-compose.yml` で依存サービスを起動し、ルートに `.env` を用意する前提が明記されている。
- API 型定義再生成手順には、API サーバ起動に `DATABASE_URL` 等の環境変数と依存サービス（PostgreSQL、MinIO）が必要である旨が明記されている。
- あわせて、OpenAPI エンドポイントを `curl` で確認する手順も追加されている。
- 前回指摘していた README 上の不足は解消済みと判断した。

### ドキュメント確認

- `README.md` 単体の確認範囲では、前回指摘に対する修正内容と整合している。
- 今回の確認では新たな README 上のブロッカーは確認していない。

### PR/完了結果

- `docs_ready: true`

### 残リスク

- 今回の依頼は README の再確認に限定されており、実行手順そのものの再試験は行っていない。

---

## ドキュメント確認フェーズ（2026-05-03）

### ドキュメント確認結果

- 対象: `README.md`、`docs/external/openapi-typescript-v7-cli.md`、`docs/logs/62/worklog.md`
- `README.md` のポート説明自体は実装と整合している。`generate:api` の参照先が `http://localhost:3000/api/v1/openapi.json` である点も `boardflow/package.json` と一致している。
- ただし `README.md` の再生成手順は、`mise exec -- cargo run -p boardflow-api` をそのまま実行すればよいように読める一方で、実際には `DATABASE_URL` 未設定で起動失敗したため、現状の記述だけでは再現不能である。
- `docs/external/openapi-typescript-v7-cli.md` は、公式 CLI / migration guide の内容と整合している。単一リモートスキーマ + `-o` の基本構文継続、`--check` フラグ追加、認証の Redocly config への移行、`defaultNonNullable: true` デフォルト化、globbing 廃止の整理はいずれも妥当。
- `docs/logs/62/worklog.md` には調査・計画・実装・再レビューの流れが記録されているが、今回のドキュメント確認で判明した README の再現性不足は未記録だったため、この節で追記した。

### 検証結果

- `cd /home/f0reach/workspace/boardflow && mise exec -- cargo run -p boardflow-api`
- 結果: `Error: MissingEnvVar("DATABASE_URL")`
- 上記により、README の API 起動手順には前提条件の明記が不足していることを確認した。

### ドキュメント判定

- `docs_ready: false`

### 必須修正

- `README.md` の Frontend ローカル開発と API 型定義再生成の手順に、API 起動前提として必要な backend 環境セットアップを追記すること。
- 少なくとも `DATABASE_URL` を含む必須環境変数の準備方法、またはそれを満たす起動手順（例: 使用する `.env` / compose 起動手順）を、`mise exec -- cargo run -p boardflow-api` の前に明示すること。

### 任意改善

- `pnpm generate:api` 実行前提として「API サーバーが `http://localhost:3000` で疎通可能であること」を README に明記すると、失敗時の切り分けがしやすい。
- 参考資料として `docs/external/openapi-typescript-fetch.md` には旧ポート記述が残っているため、別 Issue で整合更新すると混乱を減らせる。

### 残リスク

- 今回は API 起動失敗により `pnpm generate:api` の実行再確認までは行っていない。
- そのため、README に前提条件を追記した後は、API 起動から `pnpm generate:api` 完了までを通しで再検証する必要がある。
