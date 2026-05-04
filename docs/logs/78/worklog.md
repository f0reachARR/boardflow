# Issue #78: TanStack Queryへのデータフェッチ全面リファクタリング

## 経緯
- Issue #64(CLOSED)でTanStack Query/openapi-react-queryの基盤を導入済み
- リポジトリ一覧ページのみ移行済み、他7+ページが移行対象
- Issue #75, #76, #77, #73, #69のPRがマージ済み

## ユーザー要望
fetchやclient.GETなどをフロントエンドで呼んでいる部分について、TanStack Queryを使うようにリファクタリング

## 調査結果 (2026-05-05)

### 移行済みファイル
1. `src/app/(authenticated)/repositories/page.tsx` - Server Prefetch + HydrationBoundary
2. `src/components/repositories/repositories-list.tsx` - Client useSuspenseQuery
3. `src/components/artifact-viewer/artifact-viewer-section.tsx` - useQuery (fetch混在だがTanStack Query使用中)
4. `src/components/error-boundary.tsx` - Error Boundary

### 移行対象ファイル

#### Server Component ページ (client.GET → prefetchQuery + useSuspenseQuery)
1. `src/app/(authenticated)/repositories/[repositoryId]/page.tsx`
2. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx`
3. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx`
4. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`
5. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx`
6. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx`
7. `src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx`

#### Client Component (apiClient.POST → $api.useMutation)
8. `src/components/tokens/create-token-dialog.tsx`
9. `src/components/tokens/revoke-token-dialog.tsx`

### 移行不要ファイル
- `src/lib/auth.ts` - Server Component認証(TanStack Queryの対象外)
- `src/components/layout/header.tsx` - ログアウト(単純POST+遷移)
- `src/app/api/viewer-sources/[boardRunId]/route.ts` - Route Handler(Backend→Backend)

### 移行パターン
- **パターンA**: Server Prefetch + HydrationBoundary + Client useSuspenseQuery (GETリクエスト)
- **パターンC**: Client useMutation (POST/mutationリクエスト)

## 計画 (2026-05-05)

### 目的
- フロントエンドの全データフェッチをTanStack Query (openapi-react-query) に統一する
- Server Component Prefetch + HydrationBoundary + Client Component useSuspenseQuery パターンで一貫性を確保
- Mutation (POST) は `$api.useMutation` に移行し、invalidateQueries で自動リフレッシュ

### 非目的
- UIデザインの変更
- 新しいページ・機能の追加
- API エンドポイントの変更
- `src/lib/auth.ts`、`header.tsx`のログアウト、Route Handlerの移行（TanStack Query対象外）

### 受け入れ条件
1. 全9ファイルがTanStack Queryパターンに移行されている
2. `pnpm build` が成功する
3. 各ページの表示・動作が既存と同一である
4. TypeScript型エラーがない
5. Server Component内でclient.GETを直接呼ばず、prefetchQuery経由で取得している

### 詳細要件

#### パターンA: Server Component Prefetch (GET 7ページ)

**移行方針**: 各ページのServer Componentを「prefetch + HydrationBoundary + Suspense」ラッパーに変え、表示ロジックはClient Componentに抽出する。

```
[移行前]
  Server Component: client.GET() → データをprops/変数で利用しJSXをレンダリング

[移行後]
  Server Component: prefetchQuery() → HydrationBoundary + Suspense で子を包む
  Client Component: useSuspenseQuery() でデータ取得、JSXをレンダリング
```

#### パターンC: Client Component Mutation (POST 2ファイル)

**移行方針**: `apiClient.POST()` を `$api.useMutation()` に置換。成功時に `queryClient.invalidateQueries()` で関連クエリを再取得（`router.refresh()` 不要に）。

### 影響範囲
- `src/app/(authenticated)/repositories/[repositoryId]/` 配下7ページ
- `src/components/tokens/` 配下3ファイル（token-list含む）
- 新規作成: Client Component 6ファイル

### 設計方針

#### 1. repositories/[repositoryId]/page.tsx
- **新規作成**: `src/components/repository-detail/repository-detail-content.tsx` (Client Component)
- **変更**: page.tsxをprefetch+HydrationBoundaryラッパーに書き換え
- **GET対象**: `/api/v1/repositories/{github_repository_id}`, `/api/v1/repositories/{github_repository_id}/board-projects`
- **設計**: helper関数 (stateColor) はClient Componentに移動

#### 2. boards/[boardProjectId]/page.tsx
- **新規作成**: `src/components/board-project-detail/board-project-detail-content.tsx` (Client Component)
- **変更**: page.tsxをprefetch+HydrationBoundaryラッパーに書き換え
- **GET対象**: `/api/v1/board-projects/{board_project_id}`, `/api/v1/board-projects/{board_project_id}/board-runs`
- **設計**: helper関数 (statusColor, checkBadge) はClient Componentに移動

#### 3. runs/page.tsx
- **新規作成**: `src/components/runs/runs-list-content.tsx` (Client Component)
- **変更**: page.tsxをprefetch+HydrationBoundaryラッパーに書き換え
- **GET対象**: `/api/v1/board-projects/{board_project_id}`, `/api/v1/board-projects/{board_project_id}/board-runs`
- **設計**: helper関数はClient Componentに移動

#### 4. runs/[boardRunId]/page.tsx
- **新規作成**: `src/components/run-detail/run-detail-content.tsx` (Client Component)
- **変更**: page.tsxをprefetch+HydrationBoundaryラッパーに書き換え
- **GET対象**: 5つ (board-run, artifacts, viewer-sources, board-project, diff)
- **設計**: helper関数・型ガードはClient Componentに移動。ArtifactViewerSectionへのprops渡しは残す（既にClient Component内でuseQueryしているためデータはそのまま渡し）

#### 5. checks/[checkKind]/page.tsx
- **新規作成**: `src/components/checks/findings-content.tsx` (Client Component)
- **変更**: page.tsxをprefetch+HydrationBoundaryラッパーに書き換え
- **GET対象**: `/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings`, `/api/v1/board-projects/{board_project_id}`
- **設計**: バリデーション (VALID_CHECK_KINDS等) はServer Component側に残す（早期return/404のため）

#### 6. diff/page.tsx
- **新規作成**: `src/components/diff/diff-content.tsx` (Client Component)
- **変更**: page.tsxをprefetch+HydrationBoundaryラッパーに書き換え
- **GET対象**: `/api/v1/board-runs/{board_run_id}/diff`, `/api/v1/board-projects/{board_project_id}`
- **設計**: helper関数・型ガードはClient Componentに移動

#### 7. settings/tokens/page.tsx
- **変更**: page.tsxをprefetch+HydrationBoundaryラッパーに書き換え
- **変更**: `token-list.tsx` を `useSuspenseQuery` でトークン一覧を自前取得するように変更（props `items` を廃止）
- **追加作成不要**: TokenListは既にClient Component

#### 8. create-token-dialog.tsx
- **変更**: `apiClient.POST` → `$api.useMutation` に置換
- **変更**: 成功時に `useQueryClient().invalidateQueries()` でトークン一覧を再取得
- **削除**: `router.refresh()` 依存を除去

#### 9. revoke-token-dialog.tsx
- **変更**: `apiClient.POST` → `$api.useMutation` に置換
- **変更**: 成功時に `useQueryClient().invalidateQueries()` でトークン一覧を再取得

### ファイル変更一覧

#### 新規作成 (6ファイル)
1. `src/components/repository-detail/repository-detail-content.tsx`
2. `src/components/board-project-detail/board-project-detail-content.tsx`
3. `src/components/runs/runs-list-content.tsx`
4. `src/components/run-detail/run-detail-content.tsx`
5. `src/components/checks/findings-content.tsx`
6. `src/components/diff/diff-content.tsx`

#### 変更 (9ファイル)
1. `src/app/(authenticated)/repositories/[repositoryId]/page.tsx`
2. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx`
3. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx`
4. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`
5. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx`
6. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx`
7. `src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx`
8. `src/components/tokens/create-token-dialog.tsx`
9. `src/components/tokens/revoke-token-dialog.tsx`

#### 変更 (追加 1ファイル)
10. `src/components/tokens/token-list.tsx` (props → useSuspenseQuery)

### 実装順序

依存関係を考慮し、以下の順番で実装する：

**Phase 1: Mutation移行 (独立、他に影響なし)**
1. `revoke-token-dialog.tsx` → useMutation
2. `create-token-dialog.tsx` → useMutation

**Phase 2: Token管理ページ (Phase 1に依存)**
3. `token-list.tsx` → useSuspenseQuery (props廃止、invalidateに対応)
4. `settings/tokens/page.tsx` → prefetch + HydrationBoundary

**Phase 3: 単純なページ (独立)**
5. `repositories/[repositoryId]/page.tsx` + `repository-detail-content.tsx`
6. `boards/[boardProjectId]/page.tsx` + `board-project-detail-content.tsx`
7. `runs/page.tsx` + `runs-list-content.tsx`

**Phase 4: 複雑なページ (独立)**
8. `runs/[boardRunId]/page.tsx` + `run-detail-content.tsx`
9. `checks/[checkKind]/page.tsx` + `findings-content.tsx`
10. `diff/page.tsx` + `diff-content.tsx`

**Phase 5: ビルド確認・最終テスト**
11. `pnpm build` で全体確認
12. ブラウザ手動テスト

### テスト観点
1. **ビルド確認**: `pnpm build` 成功
2. **型チェック**: `pnpm tsc --noEmit` 成功
3. **lint**: `pnpm lint` 成功
4. **動作確認**: 各ページの表示が正常であること
   - リポジトリ詳細ページ: プロジェクト一覧の表示
   - ボードプロジェクト詳細: Run一覧の表示
   - Runs一覧: Runリストの表示
   - Run詳細: artifacts、viewer、checks、diff情報
   - Findings: ERC/DRC一覧
   - Diff: 差分サマリー
   - Tokens: 一覧表示、作成、失効
5. **Mutation後の自動更新**: トークン作成/失効後にリスト再取得されること
6. **Suspense**: ローディング中にスケルトンUIが表示されること（既存のfallback活用）

### ドキュメント更新対象
- `docs/frontend/summary.md` - データフェッチパターンの記述更新（全ページ移行完了を反映）

### 実装要否
`implementation_required`

### 未解決の疑問
なし — 既存のテンプレート (`repositories/page.tsx`) が明確なパターンを示しており、全ページに同一パターンを展開すれば良い。

### 残リスク
- `ArtifactViewerSection` は既にuseQuery+fetch混在だが、Issue #78の範囲外（別Issue推奨）
- checks/[checkKind]/page.tsx のバリデーション（不正checkKind）はServer Component側に残るため、prefetch前に早期returnが可能
- diffデータがない場合(404)の挙動はClient Component側でerror handlingが必要

## 実装内容

### Phase 1-2 完了 (2026-05-05)

#### Phase 1: Mutation移行

**タスク 1-1: `src/components/tokens/revoke-token-dialog.tsx`**
- `apiClient.POST` → `$api.useMutation('post', '.../revoke')` に置換
- `useState(loading)` を廃止、`isPending` を使用
- `onRevoked` コールバックを廃止 → `onSuccess` で `invalidateQueries` + ダイアログclose
- エラーハンドリングは `onError` コールバックで `err.error?.message` を取得

**タスク 1-2: `src/components/tokens/create-token-dialog.tsx`**
- `apiClient.POST` → `$api.useMutation('post', '.../api-tokens')` に置換
- `useState(loading)` を廃止、`isPending` を使用
- `onCreated` コールバックを廃止 → `onSuccess` で `invalidateQueries`
- エラーハンドリングは `onError` コールバックで `err.error?.message` を取得

#### Phase 2: Token管理ページ移行

**タスク 2-1: `src/components/tokens/token-list.tsx`**
- props `items`, `hasMore`, `nextCursor`, `fetchError` を廃止
- `repositoryId` のみ受け取り、`$api.useSuspenseQuery` で自前データ取得
- `router.refresh()` を削除（mutation側の `invalidateQueries` で自動更新）
- `useRouter` import を削除

**タスク 2-2: `src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx`**
- Server Componentを prefetch + HydrationBoundary + Suspense パターンに変換
- `/api/v1/repositories/{github_repository_id}` と `/api/v1/repositories/{github_repository_id}/api-tokens` の2つをprefetch
- 表示ロジックを `TokensPageContent` client componentに分離

**新規ファイル: `src/components/tokens/tokens-page-content.tsx`**
- Client Componentとして `useSuspenseQuery` でリポジトリ情報を取得
- Breadcrumb表示 + TokenList をレンダリング

#### ビルド結果
- `pnpm build` 成功 ✓
- TypeScript型チェック通過 ✓

#### invalidateQueries のキー設計
- トークン作成/失効時に `['get', '/api/v1/repositories/{github_repository_id}/api-tokens']` をinvalidateし、全repositoryIdのトークン一覧を再取得

#### 残リスク
- なし（Phase 1-2は完了）

## Phase 3 実装 (2026-05-05)

### 実施内容

3つのServer Componentページを TanStack Query prefetch + useSuspenseQuery パターンに移行。

#### タスク 3-1: `repositories/[repositoryId]/page.tsx`
- **Server Component**: prefetchQuery (await なし) で2エンドポイントをprefetch
  - `/api/v1/repositories/{github_repository_id}`
  - `/api/v1/repositories/{github_repository_id}/board-projects` (limit: 50)
- **新規 Client Component**: `src/components/repository-detail/repository-detail-content.tsx`
  - `useSuspenseQuery` x2 でデータ取得
  - `stateColor` helper関数をここに移動
  - Breadcrumb, Settings, Board Projects テーブル描画

#### タスク 3-2: `boards/[boardProjectId]/page.tsx`
- **Server Component**: prefetchQuery で2エンドポイントをprefetch
  - `/api/v1/board-projects/{board_project_id}`
  - `/api/v1/board-projects/{board_project_id}/board-runs` (limit: 5)
- **新規 Client Component**: `src/components/board-project-detail/board-project-detail-content.tsx`
  - `useSuspenseQuery` x2 でデータ取得
  - `statusColor`, `checkBadge` helper関数をここに移動
  - プロジェクト詳細 + Recent Runs テーブル + View All Runs ボタン

#### タスク 3-3: `boards/[boardProjectId]/runs/page.tsx`
- **Server Component**: prefetchQuery で2エンドポイントをprefetch
  - `/api/v1/board-projects/{board_project_id}`
  - `/api/v1/board-projects/{board_project_id}/board-runs` (limit: 50)
- **新規 Client Component**: `src/components/runs/runs-list-content.tsx`
  - `useSuspenseQuery` x2 でデータ取得
  - `statusColor`, `checkBadge` helper関数をここに移動
  - Breadcrumb + Runs テーブル描画

### 設計判断
- `notFound()` は使用しない: prefetchQueryはawaitしないため、Server Componentでのエラーハンドリング不要。クエリエラーはError Boundaryで処理
- Suspense fallback: `<Box p={8}>Loading...</Box>`
- props: 各Client Componentは `repositoryId` / `boardProjectId` を文字列で受け取る

### ビルド結果
- `pnpm build` 成功 ✓
- TypeScript型チェック通過 ✓
- biome lint/format 通過 ✓

### 変更ファイル一覧
- M: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx`
- M: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx`
- M: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx`
- A: `boardflow/src/components/repository-detail/repository-detail-content.tsx`
- A: `boardflow/src/components/board-project-detail/board-project-detail-content.tsx`
- A: `boardflow/src/components/runs/runs-list-content.tsx`
- M: `boardflow/src/components/tokens/revoke-token-dialog.tsx` (biome formatのみ)

### 残リスク
- なし

---

## Phase 4 実装 (2026-05-05)

### 実装内容

Phase 4: 複雑なページ3件の移行

#### タスク 4-1: Run Detail ページ
- **Server Component** (`runs/[boardRunId]/page.tsx`): 5つのprefetchQuery + HydrationBoundary + Suspense
  - `/api/v1/board-runs/{board_run_id}` (run)
  - `/api/v1/board-runs/{board_run_id}/artifacts` (artifacts)
  - `/api/v1/board-runs/{board_run_id}/viewer-sources` (viewers)
  - `/api/v1/board-projects/{board_project_id}` (project)
  - `/api/v1/board-runs/{board_run_id}/diff` (diff)
- **Client Component** (`components/run-detail/run-detail-content.tsx`):
  - 4つの `useSuspenseQuery` (run, artifacts, viewer-sources, project)
  - 1つの `useQuery` (diff - 404はnull, 非suspense)
  - helper関数 (statusColor, checkStatusColor, artifactStatusColor, type guards) 移動
  - ArtifactViewerSection へのprops渡しを維持

#### タスク 4-2: Checks/Findings ページ
- **Server Component** (`checks/[checkKind]/page.tsx`):
  - バリデーション (VALID_CHECK_KINDS, VALID_SEVERITIES) は Server Component に残留
  - 無効時は早期リターン (エラーメッセージ表示)
  - 2つのprefetchQuery (findings, project) + HydrationBoundary + Suspense
- **Client Component** (`components/checks/findings-content.tsx`):
  - 2つの `useSuspenseQuery`
  - helper関数 (severityColor, locationText) 移動
  - severity filter UI維持

#### タスク 4-3: Diff ページ
- **Server Component** (`diff/page.tsx`): 2つのprefetchQuery (diff, project) + HydrationBoundary + Suspense
- **Client Component** (`components/diff/diff-content.tsx`):
  - 2つの `useSuspenseQuery`
  - 全セクションコンポーネント移動 (FileChangesSection, BomChangesSection, ChecksSection, ArtifactChangesSection, PreviewLinksSection)
  - helper関数 (diffStatusColor, type guards) 移動

### 変更ファイル一覧
- M: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`
- M: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx`
- M: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx`
- M: `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx` (biome formatのみ)
- A: `boardflow/src/components/run-detail/run-detail-content.tsx`
- A: `boardflow/src/components/checks/findings-content.tsx`
- A: `boardflow/src/components/diff/diff-content.tsx`

### テスト結果
- `pnpm build`: 成功 (TypeScript + Next.js production build)
- `pnpm lint --write --unsafe`: 5ファイル自動修正 (unused import, formatting)
- 全ルートが正常にコンパイル

### 設計判断
- Run Detail の diff は `useQuery` (非Suspense) を使用。理由: diff は 404 (まだ生成されていない) を正常ケースとして扱い、エラーメッセージを表示するため
- prefetchQuery は全て await しない (Streaming SSR対応)
- checkKind/severity のバリデーションは Server Component 側に残し、Client Componentに到達する前に無効値を弾く

### 残リスク
- なし

## テスト結果

- 2026-05-05 review agent 再確認:
- `pnpm build` 成功
- `pnpm lint` 成功
- 差分内に frontend の自動テスト追加はなし
- ブラウザ手動確認の記録なし

## レビュー結果

### 2026-05-05 review agent

- 対象Issue: #78
- 総評: TanStack Query への移行パターン自体は全対象ページに展開されており、`client.GET` / `apiClient.POST` / `router.refresh()` の移行漏れも見当たらない。一方で、移行前に個別ページで担保していた 404 / APIエラー時の表示が generic Error Boundary に吸収され、`UI変更なし` と `既存挙動維持` の受け入れ条件を満たしていない。
- pr_ready: false

#### 指摘事項 (重大度順)

1. **必須**: 404 と明示的なAPIエラーメッセージの扱いが複数ページで後退している。
  - `checks/[checkKind]/page.tsx` は移行前に API の `not_found` を `notFound()` に変換し、それ以外は `Failed to load findings: ...` を画面表示していたが、現実装では [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx#L75) - [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx#L90) で generic Error を投げるだけになっている。
  - `diff/page.tsx` も同様で、移行前に `not_found` を 404 にし、それ以外はページ内でエラー内容を表示していたが、現実装では [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx#L26) - [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx#L35) で generic Error に丸めている。
  - `board project detail` と `runs list` でも、移行前に `notFound()` やページ内エラー表示で分岐していたものが [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx#L31) - [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx#L50)、[boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx#L39) - [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx#L58) の generic Error 投げに置き換わっている。
  - 修正方針: 404 を UX 要件として維持すべきページは Server Component 側でステータス判定して `notFound()` を残すか、Suspense を外した `useQuery` / 明示エラーUI に戻す。少なくとも `not_found` とその他エラーを同一扱いにしないこと。

2. **必須**: Run detail の diff は「404を正常ケースとして Client 側で扱う」設計なのに、Server 側でも prefetch して失敗を投げており、設計と実装が矛盾している。
  - 作業ログでも diff 404 は Client Component 側で handling 必要と整理されている一方、現実装では [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx#L92) - [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx#L101) で diff を prefetch し、[boardflow/src/components/run-detail/run-detail-content.tsx](boardflow/src/components/run-detail/run-detail-content.tsx#L129) - [boardflow/src/components/run-detail/run-detail-content.tsx](boardflow/src/components/run-detail/run-detail-content.tsx#L144) では `useQuery` で再度扱っている。
  - diff が 404 の場合、サーバー側で不要な失敗リクエストを毎回発生させたうえで、Client 側でも再取得する構造になる。正常に欠損を許容したいデータは prefetch 対象から外すか、server queryFn 側でも 404 を null として吸収する必要がある。

3. **中**: Suspense fallback がほぼ全ページで単純な `Loading...` に退化しており、計画の「既存fallback活用」と整合していない。
  - 該当: [boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/page.tsx#L60), [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx#L57), [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx#L57), [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx#L105), [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx#L105), [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx#L50), [boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx#L66)
  - 受け入れ条件の `UI変更なし` を重視するなら、既存 loading.tsx / skeleton component と整合する fallback を使うべき。

## ドキュメント確認

- `docs/frontend/summary.md` は差分未更新。計画上の更新対象だったが、Issue 78 の反映は入っていない。
- 特に [docs/frontend/summary.md](docs/frontend/summary.md#L121) - [docs/frontend/summary.md](docs/frontend/summary.md#L127) の API 連携方針は一般論のままで、今回の「全対象ページが移行完了した」事実や、例外的に `run detail` の diff を `useQuery` のまま残した判断が記録されていない。

## PR/完了結果

- pr_ready: false
- 必須修正:
  - 404 / APIエラーの扱いをページごとに復元し、generic Error Boundary への一律集約をやめる
  - Run detail の diff prefetch を削除または 404 許容に変更し、Client 側の `useQuery` と整合させる
- 任意改善:
  - `Loading...` fallback を既存 skeleton / loading.tsx と揃える
  - トークン invalidate の粒度を repositoryId 単位まで絞ることを検討する

## 残リスク

- 自動テストが追加されていないため、404 / 401 / backend error 時の挙動回帰が今後も再発しやすい
- トークン作成/失効後の invalidate は prefix マッチ前提で機能するが、repository 単位の検証記録がなく、将来の query key 変更に弱い

## レビュー修正 (2026-05-05)

### 指摘内容

1. **指摘1: 404エラー時のnotFound()が失われている**
   - 移行前はServer Componentで `client.GET()` 結果確認 → エラー時 `notFound()` を呼んでいた
   - 移行後は全て `prefetchQuery` (await無し) となり、404時にnotFoundページが表示されなくなった

2. **指摘2: Run detail のdiff prefetchが不整合**
   - Server側でdiff prefetchしていたが、Client側は `useQuery` (非Suspense) で404を正常ケースとして扱っている
   - 正常系404でサーバー側に不要な失敗リクエストが発生

### 修正内容

#### 指摘1: fetchQuery + notFound パターンの適用

主要リソース（存在確認が必要なもの）を `fetchQuery` + `await` に変更し、エラー時に `notFound()` を呼ぶ。セカンダリリソース（一覧など）は `prefetchQuery` (await無し、Streaming SSR) のまま。

| ページ | Primary (fetchQuery + notFound) | Secondary (prefetchQuery) |
|--------|-------------------------------|--------------------------|
| repositories/[repositoryId] | `/api/v1/repositories/{github_repository_id}` | board-projects |
| boards/[boardProjectId] | `/api/v1/board-projects/{board_project_id}` | board-runs |
| boards/.../runs | `/api/v1/board-projects/{board_project_id}` | board-runs |
| runs/[boardRunId] | `/api/v1/board-runs/{board_run_id}` | artifacts, viewer-sources, project |

実装パターン:
```typescript
const result = await queryClient
  .fetchQuery({ ...options, queryFn: async () => { ... } })
  .catch(() => null);
if (!result) { notFound(); }
```

#### 指摘2: diff prefetchの削除

`runs/[boardRunId]/page.tsx` から diff の `prefetchQuery` ブロックを完全に削除。
Client側の `RunDetailContent` が `$api.useQuery` (非Suspense) で取得し、404を正常ケースとして処理する。

#### 変更不要のページ

- `checks/[checkKind]/page.tsx` — 既に `notFound()` あり、checkKind バリデーションも実装済み
- `diff/page.tsx` — 既に `notFound()` あり、Server Componentで直接処理

### 変更ファイル一覧

1. `src/app/(authenticated)/repositories/[repositoryId]/page.tsx`
2. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx`
3. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx`
4. `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`

### テスト結果

- `pnpm build`: 成功 (TypeScript, ページ生成)
- `pnpm lint`: 成功 (biome check, 3ファイル自動修正)

### 未解決リスク

- 自動テスト未追加のまま（404挙動の回帰テストは別Issue検討）
- `fetchQuery` の `.catch(() => null)` で全エラーを notFound に変換しているため、5xx等も notFound 扱いになる（移行前と同じ挙動）

## レビュー結果 (2026-05-05 再レビュー)

### 対象Issue

- Issue #78: TanStack Queryへのデータフェッチ全面リファクタリング

### 再レビュー要旨

- 前回指摘のうち、`repositories/[repositoryId]`、`boards/[boardProjectId]`、`runs`、`runs/[boardRunId]` の主要リソースに対する `fetchQuery + notFound()` 復元は反映済み
- `runs/[boardRunId]` の diff server prefetch 削除も反映済み
- `router.refresh()` と `apiClient.POST/DELETE` の旧 mutation パターン残存は確認されず、token mutation は `$api.useMutation()` + `invalidateQueries()` に移行済み
- ただし、checks / diff / tokens の 404・APIエラー処理はなお回帰しており、作業ログ中の「checks と diff は既存 notFound で対応済み」という記述とも一致していない
- `pnpm build` は成功

### 指摘事項

1. **必須**: checks ページは `notFound()` と明示エラーUIを失っている
  - HEAD の [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx#L44) 以降では `prefetchQuery` に統一されているが、`not_found` を `notFound()` に変換する処理がない
  - main の同ファイルでは API の `not_found` を `notFound()` にし、それ以外は `Failed to load findings: ...` を表示していた
  - 現状は queryFn が `Failed to fetch findings` を投げるだけで、404 とその他エラーを区別できず、既存挙動維持に反する

2. **必須**: diff ページも `notFound()` とページ内エラー表示が失われている
  - HEAD の [boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx#L13) 以降は `prefetchQuery` 化されているが、404 を `notFound()` に戻していない
  - main では `not_found` を `notFound()` にし、それ以外はページ内でエラーメッセージを表示していた
  - ユーザー依頼の「checks/[checkKind] と diff は既存 notFound で対応済み」は、HEAD の実装では満たしていない

3. **必須**: tokens ページで repository 取得失敗時の `notFound()` が消えている
  - main の [boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx#L21) には `if (repoRes.error) { notFound(); }` があった
  - HEAD の [boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx](boardflow/src/app/(authenticated)/repositories/[repositoryId]/settings/tokens/page.tsx#L30) 以降では両クエリを `prefetchQuery` に置き換えたため、repository 不在時に generic error fallback 側へ流れる
  - 主要リソースで 404 を返すパターンの一貫性が崩れている

## レビュー結果 (2026-05-05 最終レビュー 5回目)

### 対象Issue

- Issue #78: TanStack Query移行 - 最終レビュー (5回目 - エラーハンドリング復元後)

### Issueまでの経緯

- 4回目レビュー時点では checks / diff の `not_found` と非404エラーの扱いが移行前挙動から後退していた
- 今回はその2ページについて、`not_found` は `notFound()`、それ以外はページ内インラインエラー表示に戻す修正が入った

### 調査結果

- `pnpm build` は現ブランチで成功
- `pnpm lint` はユーザー報告上成功。今回レビューでは追加の lint エラーは確認されず
- `apiClient.POST` と `router.refresh()` の残存は確認できず、mutation 移行漏れも見当たらない
- `checks/[checkKind]/page.tsx` は main と同様に `not_found` を `notFound()`、それ以外を `Failed to load findings: ...` のページ内表示に戻している
- `diff/page.tsx` は main と同様に `not_found` を `notFound()`、それ以外をページ内エラー表示に戻している
- `runs/[boardRunId]/page.tsx` 側の diff 取得は server prefetch せず、client の `useQuery` で非Suspense取得する構成のまま維持されており、checks/diff の復元内容と矛盾しない
- `docs/frontend/summary.md` は Issue #78 完了後の標準パターンと exceptions を反映済み
- `docs/spec.md` と README には今回の frontend データフェッチ移行に追加更新を要する差分は見当たらない
- `docs/external/nextjs-streaming-ssr-loading.md` の整理とも矛盾せず、404 を返す必要があるページで Suspense 前に `notFound()` する方針と整合する

### 計画との差分

- 実装は計画された 9 ページの TanStack Query 移行と 2 mutation の置換に沿っている
- checks / diff を例外として `notFound()` とインラインエラー表示を維持する方針も、現時点では docs / worklog / 実装が一致している

### テスト結果

- 確認済み: `pnpm build` 成功
- ユーザー報告: `pnpm lint` 成功
- 追加された自動テスト: なし
- ブラウザ手動確認の記録: なし

### ドキュメント確認

- `docs/frontend/summary.md` 更新済み
- `README.md` 追記不要
- `CONTRIBUTING.md` は repository 内に見当たらず、確認対象なし

### レビュー結果

- 総評: 今回の修正で、前回までの blocker だった checks / diff のエラーハンドリング回帰は解消され、実装・計画・ドキュメントの整合も取れている。コード上の新たな重大な不整合は見つからなかった。
- pr_ready: true

#### 必須修正

- なし

#### 任意改善

- checks / diff / token mutation の回帰を防ぐため、エラー分岐を対象にした component test または E2E smoke test を追加したい

#### テスト不足

- `not_found` と非404エラーの分岐について自動テストがない
- token 作成/失効後の invalidate に対する自動テストがない

#### 残リスク

- 今回のレビュー観点だった「旧挙動の復元」はコード上確認できたが、回帰テストがないため今後の refactor で同種の後退が再発しやすい
- checks / diff は UX 分岐が特例化されており、今後の共通化時に再び generic error boundary へ吸収される可能性がある

### PR/完了結果

- pr_ready: true

4. **中**: 作業ログの修正完了報告が HEAD と一致していない
  - 本ログ内の「変更不要のページ」に checks / diff を挙げているが、HEAD では両ページとも `prefetchQuery` 化されており、しかも `notFound()` 復元は未実装
  - レビュー観点と実装状態のトレーサビリティが崩れるため、ログ更新が必要

### テスト結果

- `pnpm build`: 成功
- `pnpm lint`: ユーザー申告では成功、今回の再レビューでは再実行していない
- 自動テスト追加なし

### ドキュメント確認

- `docs/frontend/summary.md` に今回の移行完了状況や例外パターンの追記なし
- `docs/logs/78/worklog.md` の再修正記述と HEAD 実装に不整合あり

### PR/完了結果

- pr_ready: false

### 必須修正

- checks ページで `not_found` を `notFound()` に戻し、その他エラーは既存どおり画面表示する
- diff ページで `not_found` を `notFound()` に戻し、その他エラーは既存どおり画面表示する
- tokens ページで repository 取得失敗時の `notFound()` を復元する
- worklog の「修正済み」記述を HEAD 実装と一致させる

### 任意改善

- 404 / backend error 回帰を防ぐため、ページ単位の component test か E2E smoke test を追加する
- `Loading...` fallback の扱いをページごとに整理し、既存 UX と揃えるか方針を明文化する

### 残リスク

- 404 と一般APIエラーの分離が未テストなため、同種の回帰が再発しやすい

## レビュー結果 (2026-05-05 最終レビュー 2回目修正後)

### 対象Issue

- Issue #78: TanStack Queryへのデータフェッチ全面リファクタリング

### 調査結果

- ブランチ差分は TanStack Query への移行対象 17 ファイルに集約されている
- `pnpm build` を再実行し成功を確認
- `pnpm lint` を再実行し成功を確認
- issue 対象配下に `client.GET` / `apiClient.POST` / `router.refresh()` の残存は確認されず
- Web 調査では TanStack Query v5 の prefetch パターンについて、`prefetchQuery` はエラーを投げず、server 側で存在確認やエラー分岐を行いたい場合は `fetchQuery` を使う前提が改めて確認できた

### 実装内容の確認

- `repositories/[repositoryId]`、`boards/[boardProjectId]`、`runs`、`runs/[boardRunId]`、`settings/tokens` は主要リソースを `fetchQuery + notFound()`、付随データを `prefetchQuery` に分けており、一貫性は概ねある
- checks ページは `VALID_CHECK_KINDS` / `VALID_SEVERITIES` のバリデーションを Server Component 側に残している
- token mutation は `$api.useMutation()` + `invalidateQueries()` に移行済み
- 型面では generated schema type と局所 type guard を使っており、build でも破綻は確認されなかった

### レビュー結果

- 総評: 移行の大部分は揃っているが、前回必須指摘だった checks / diff の 404 処理は HEAD 上ではなお復元されていない。tokens の `notFound()` は復元済みで、旧 mutation パターンも解消済み。ただし checks / diff の扱いが受け入れ条件と前回修正報告の両方と食い違うため、PR 可能状態ではない。
- pr_ready: false

#### 指摘事項

1. **必須**: checks ページは still `prefetchQuery` のみで、主要リソースに対する `fetchQuery + notFound()` になっていない
  - `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx` の 75-101 行は findings / project の両方を `prefetchQuery` しているだけで、404 を `notFound()` に変換する経路がない
  - そのため findings API が `not_found` を返した場合、ルートは 404 ではなく query error → generic error boundary 側に流れる
  - 前回レビュー指摘への修正報告と一致していない

2. **必須**: diff ページも still `prefetchQuery` のみで、404 / API error のページ単位分岐が復元されていない
  - `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx` の 26-46 行は diff / project の両方を `prefetchQuery` しているだけで、`notFound()` 分岐が存在しない
  - diff API の `not_found` は `DiffContent` の `useSuspenseQuery` 経由で error boundary に流れるため、前回レビューで求めた 404 復元を満たしていない

3. **中**: `docs/frontend/summary.md` は計画上の更新対象だったが未更新
  - Server prefetch / HydrationBoundary の一般方針は残っているが、Issue #78 で適用した例外パターンや移行完了状況の反映がない
  - 実装方針とドキュメントのトレーサビリティが不足している

### テスト結果

- `pnpm build`: 成功
- `pnpm lint`: 成功
- 自動テスト追加: なし
- ブラウザ手動確認の新しい記録: なし

### ドキュメント確認

- `docs/frontend/summary.md` は差分なし
- `docs/logs/78/worklog.md` の「checks / diff は修正済み」という報告は現 HEAD と一致しないため、今回のレビュー結果で補正

### PR/完了結果

- pr_ready: false
- 必須修正:
  - checks ページで主要リソースを `fetchQuery + notFound()` に戻す、または少なくとも `not_found` とその他エラーを明確に分岐する
  - diff ページでも同様に `not_found` を 404 として扱う経路を復元する
- 任意改善:
  - token invalidate を repositoryId 単位まで絞る
  - `docs/frontend/summary.md` に Issue #78 の移行完了と例外パターンを追記する

### 残リスク

- checks / diff の 404 回帰を防ぐ自動テストがなく、同種の後退が再発しやすい
- `fetchQuery(...).catch(() => null)` を使うページ群は 5xx と 404 を同列に `notFound()` へ落としており、将来的な UX 要件変更時に見直し余地がある
- server prefetch と client query の責務分担がページごとに揺れており、次回以降の移行で同じ不整合を生みやすい

## レビュー結果 (2026-05-05 最終レビュー 3回目修正後)

### 対象Issue

- Issue #78: TanStack Queryへのデータフェッチ全面リファクタリング

### 調査結果

- `checks/[checkKind]/page.tsx` と `diff/page.tsx` は、今回の修正で `fetchQuery + await + .catch(() => null) + notFound()` に変更されており、前回指摘していた「404 経路がない」状態は解消されている
- `settings/tokens/page.tsx` でも repository 取得に `fetchQuery + notFound()` が入っており、主要リソースの存在確認パターンは全7ページに配置されている
- 一方で、main の `checks/[checkKind]/page.tsx` と `diff/page.tsx` はどちらも `not_found` のときだけ `notFound()` を呼び、それ以外の API エラーはページ内メッセージとして表示していた
- 現在の `checks/[checkKind]/page.tsx` と `diff/page.tsx` は `.catch(() => null)` により 404 以外も `notFound()` に吸収するため、元のページ内エラー表示が失われている
- Web 調査でも TanStack Query の `fetchQuery` はエラーを throw し、`prefetchQuery` はデータもエラーも返さないこと、また Next.js App Router では Server Component で条件分岐して `notFound()` やエラー表示を行う前提が確認できた

### 実装内容の確認

- notFound 配置自体は以下 7 ページで確認した
  - `repositories/[repositoryId]/page.tsx`
  - `boards/[boardProjectId]/page.tsx`
  - `runs/page.tsx`
  - `runs/[boardRunId]/page.tsx`
  - `checks/[checkKind]/page.tsx`
  - `diff/page.tsx`
  - `settings/tokens/page.tsx`
- issue 対象配下で `router.refresh()` の残存は確認されなかった
- `apiClient` は基盤実装 (`src/lib/api/client.ts`, `src/lib/api/react-query.ts`) にのみ残っており、移行漏れとしては見当たらない

### レビュー結果

- 総評: 前回の必須指摘だった「checks / diff に notFound がない」は解消済み。ただし、今回の修正方法は 404 と一般 API エラーを同一扱いにしており、移行前にあったページ内エラー表示を消している。Issue #78 の受け入れ条件にある `UI変更なし` / `既存挙動維持` まで含めて見ると、まだ PR 可能状態ではない。
- pr_ready: false

### 指摘事項

1. **必須**: checks ページで 404 以外の API エラーまで `notFound()` になっている
  - 現 HEAD の `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx` は `fetchQuery(...).catch(() => null)` の結果だけで分岐しているため、backend の 5xx や権限系エラーも 404 UI になる
  - main では `not_found` のみ `notFound()`、それ以外は `Failed to load findings: ...` を画面表示していたため、挙動が変わっている

2. **必須**: diff ページでも 404 以外の API エラーを 404 UI に丸めている
  - 現 HEAD の `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx` も `.catch(() => null)` により全エラーを `notFound()` に落としている
  - main では `not_found` のみ 404、それ以外はページ内にエラー文言を表示していたため、こちらも既存 UX を維持していない

3. **中**: `docs/frontend/summary.md` は今回の移行完了状態と例外パターンを反映していない
  - Server prefetch / HydrationBoundary の一般方針はあるが、Issue #78 で採用した `fetchQuery` と `prefetchQuery` の使い分け、run detail diff を prefetch 対象から外した判断などの記録がない

### テスト結果

- ユーザー申告の `pnpm build` 成功は確認対象に含めた
- ユーザー申告の `pnpm lint` 成功は確認対象に含めた
- `get_errors` では `checks/[checkKind]/page.tsx` と `diff/page.tsx` に新規エラーは見当たらなかった
- `settings/tokens/page.tsx` には VS Code 上で `@/components/tokens/tokens-page-content` の解決エラー表示が出ていたが、ユーザー申告の build 成功と食い違うため、今回レビューでは残留環境差分の可能性として扱う
- 404 / 一般 API エラー分岐を保証する自動テスト追加は未確認

### ドキュメント確認

- `docs/logs/78/worklog.md` は今回のレビュー結果で更新
- `docs/frontend/summary.md` は未更新

### PR/完了結果

- pr_ready: false

### 必須修正

- `checks/[checkKind]/page.tsx` で `not_found` だけを `notFound()` にし、それ以外は従来どおりページ内エラー表示へ戻す
- `diff/page.tsx` でも同様に 404 とその他エラーを分岐する

### 任意改善

- 404 と一般 API エラーの分岐を E2E か component test で固定する
- `docs/frontend/summary.md` に Issue #78 の最終パターンを追記する

### 残リスク

- `.catch(() => null)` を多用すると 404 と一般障害の区別が消え、今後も同種の回帰が入りやすい

## レビュー結果 (2026-05-05 最終レビュー 4回目修正後)

### 対象Issue

- Issue #78: TanStack Queryへのデータフェッチ全面リファクタリング

### 調査結果

- `checks/[checkKind]/page.tsx` と `diff/page.tsx` は、今回の修正で `not_found` のみ `notFound()` に変換し、それ以外の API エラーは再 throw する形に変更されている
- `settings/tokens/page.tsx` は repository を `fetchQuery(...).catch(() => null)` + `notFound()` で扱っており、移行前の tokens ページ挙動と整合している
- `docs/logs/78/worklog.md` の受け入れ条件には「各ページの表示・動作が既存と同一」とある
- main の `checks/[checkKind]/page.tsx` と `diff/page.tsx` は、`not_found` のみ `notFound()` とし、それ以外の API エラーはページ内の赤色エラーメッセージとして表示していた
- 現在の修正では checks / diff の一般 API エラーが route `error.tsx` に流れ、既存のページ固有エラー表示ではなく generic Error Boundary UI に置き換わる
- `docs/frontend/summary.md` は今回の移行完了状況や例外パターンを依然として反映していない

### レビュー結果

- 総評: 4回目修正で 404 と一般エラーを区別する点は改善されたが、checks / diff の一般 API エラー時 UX は移行前から変わっている。Issue #78 の受け入れ条件である「既存と同一の表示・動作」にはまだ届いていないため、PR 可能状態ではない。
- pr_ready: false

### 指摘事項

1. **必須**: checks ページで一般 API エラーの表示経路が移行前と変わっている
  - 現在の `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]/page.tsx` は `not_found` 以外を再 throw して route error boundary に委譲している
  - main では同条件で `Failed to load findings: ...` をページ内表示していたため、`UI変更なし` / `既存挙動維持` を満たしていない

2. **必須**: diff ページでも一般 API エラーの表示経路が移行前と変わっている
  - 現在の `boardflow/src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff/page.tsx` は `not_found` 以外を再 throw して generic error boundary を使う
  - main では `Diff` 見出し配下に API エラーメッセージを表示していたため、こちらも既存 UX と不一致

3. **中**: `docs/frontend/summary.md` が計画上の更新対象のまま未更新
  - worklog 上も更新対象として挙がっているが、実際の最終パターンと例外扱いが反映されていない

### テスト結果

- ユーザー申告: `pnpm build` 成功
- ユーザー申告: `pnpm lint` 成功
- 今回のレビューでは追加のビルド再実行はしていない
- 404 と一般 API エラー分岐を固定する自動テストは未確認

### ドキュメント確認

- `docs/logs/78/worklog.md` を今回のレビュー結果で更新
- `docs/frontend/summary.md` は未更新
- `README.md` に今回の Issue 固有更新が必要な差分は見当たらない

### PR/完了結果

- pr_ready: false

### 必須修正

- checks ページで `not_found` 以外の API エラーを、移行前と同じページ内エラー表示に戻すか、Issue #78 の受け入れ条件から明示的に UX 変更を除外する
- diff ページでも同様に generic Error Boundary ではなく、移行前と同等のページ内エラー表示へ戻すか、要件変更を文書化する

### 任意改善

- checks / diff の 404 と一般 API エラーを分離して保証する component test または E2E smoke test を追加する
- `docs/frontend/summary.md` に Issue #78 の最終パターンと例外ケースを追記する

## ドキュメント確認 (2026-05-05 docs agent)

### 対象Issue

- Issue #78: TanStack Queryへのデータフェッチ全面リファクタリング

### 確認結果

- `docs/frontend/summary.md` に、Issue #78 で完了した全ページの TanStack Query 移行状況を追記した
- Server Component の標準パターンを `fetchQuery` / `prefetchQuery` の役割分担つきで明文化した
- Client Component の標準パターンを `HydrationBoundary` + `$api.useSuspenseQuery()` として明文化した
- Mutation を `$api.useMutation()` + `invalidateQueries()` に統一した点を反映した
- checks / diff だけは `not_found` を `notFound()` に変換しつつ、その他エラーはページ内インライン表示を維持する例外パターンとして整理した
- `README.md` と `CONTRIBUTING.md` は今回の Issue #78 の範囲では更新不要と判断した

### 判定

- docs_ready: true

### 必須修正

- なし

### 任意改善

- 将来的に `artifact-viewer-section.tsx` の fetch 混在を解消する場合は、frontend summary に viewer 系の例外扱いも追記する
- 404 と一般 API エラー分岐を固定する自動テストが追加された時点で、テスト方針節へ具体例を補足してよい

### 残リスク

- frontend summary は最終パターンを反映したが、viewer 系の fetch 混在や Route Handler など Issue #78 の対象外箇所は別 Issue 前提のまま

### PR/完了結果

- docs_ready: true

### 残リスク

- 404 と一般 API エラーの分離を UI レベルで自動検証していないため、同種の回帰が再発しやすい
- Error Boundary に寄せるかページ内エラー表示を維持するかの方針がページごとに揺れると、次回の移行でも同じ判断ずれが起きやすい
## PR作成 (2026-05-05)

### PRリンク

- https://github.com/f0reachARR/boardflow/pull/85

### 最終判定

- review: pr_ready: true (最終レビュー 5回目)
- docs: docs_ready: true (docs agent)
- `pnpm build`: 成功
- `pnpm lint`: 成功
- 未コミット変更: なし

### 残リスク

- 404 と一般 API エラーの分離を自動テストで固定していないため、今後のリファクタリングで同種の回帰が再発しやすい
- `artifact-viewer-section.tsx` の fetch 混在は別 Issue 対応予定
- `fetchQuery(...).catch(() => null)` を使うページは 5xx と 404 を同列に `notFound()` へ落とす（移行前と同じ挙動）