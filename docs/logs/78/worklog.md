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

(impl agent完了後に記載)

## テスト結果

(impl agent完了後に記載)

## レビュー結果

(review agent完了後に記載)

## ドキュメント確認

(docs agent完了後に記載)

## PR/完了結果

(pr agent完了後に記載)

## 残リスク

(完了後に記載)
