# Issue #108: ルート生成を routes.ts に集約

## Issueまでの経緯

- #107（format helpers集約）、#98（pagination cursor refactoring）がマージ済み
- フロントエンドのリファクタリングシリーズの一環として、ルート文字列のハードコード重複を解消する

## ユーザー要望

`boardflow/src/lib/routes.ts` にルート生成関数を集約し、コンポーネント内のテンプレートリテラルによるルート文字列の重複を排除する。

## 調査結果（リサーチフェーズ 2026-05-14）

### 既存の routes.ts

`boardflow/src/lib/routes.ts` は **存在しない**。新規作成が必要。

### Next.js ルーティング構造

```
/login
/repositories
/repositories/[repositoryId]
/repositories/[repositoryId]/settings/tokens
/repositories/[repositoryId]/boards/[boardProjectId]
/repositories/[repositoryId]/boards/[boardProjectId]/runs
/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]
/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/checks/[checkKind]
/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/diff
```

### ハードコードされたルートパターン一覧

#### 1. `/repositories`（静的、10箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/middleware.ts` | 13 | redirect |
| `src/app/login/page.tsx` | 14 | redirect |
| `src/components/layout/sidebar.tsx` | 8 | NAV_ITEMS |
| `src/components/repository-detail/repository-detail-content.tsx` | 38 | breadcrumb |
| `src/components/board-project-detail/board-project-detail-content.tsx` | 39 | breadcrumb |
| `src/components/checks/findings-content.tsx` | 81 | breadcrumb |
| `src/components/runs/runs-list-content.tsx` | 39 | breadcrumb |
| `src/components/run-detail/run-detail-content.tsx` | 76 | breadcrumb |
| `src/components/tokens/tokens-page-content.tsx` | 25 | breadcrumb |
| `src/components/diff/diff-content.tsx` | 42 | breadcrumb |

#### 2. `/login`（静的、5箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/middleware.ts` | 4, 15, 25 | PUBLIC_PATHS / redirect |
| `src/app/(authenticated)/layout.tsx` | 9 | redirect |
| `src/components/layout/header.tsx` | 55 | window.location.href |

#### 3. `` `/repositories/${repositoryId}` ``（動的、8箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/components/repositories/repositories-list.tsx` | 49 | Link href |
| `src/components/board-project-detail/board-project-detail-content.tsx` | 42, 64 | breadcrumb, Link href |
| `src/components/run-detail/run-detail-content.tsx` | 79 | breadcrumb |
| `src/components/runs/runs-list-content.tsx` | 42 | breadcrumb |
| `src/components/checks/findings-content.tsx` | 84 | breadcrumb |
| `src/components/diff/diff-content.tsx` | 45 | breadcrumb |
| `src/components/tokens/tokens-page-content.tsx` | 26 | breadcrumb |

#### 4. `` `/repositories/${repositoryId}/settings/tokens` ``（動的、1箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/components/repository-detail/repository-detail-content.tsx` | 66 | Link href |

#### 5. `` `/repositories/${repositoryId}/boards/${boardProjectId}` ``（動的、5箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/components/repository-detail/repository-detail-content.tsx` | 96 | Link href |
| `src/components/run-detail/run-detail-content.tsx` | 83 | breadcrumb |
| `src/components/runs/runs-list-content.tsx` | 46 | breadcrumb |
| `src/components/checks/findings-content.tsx` | 88 | breadcrumb |
| `src/components/diff/diff-content.tsx` | 49 | breadcrumb |

#### 6. `` `/repositories/${repositoryId}/boards/${boardProjectId}/runs` ``（動的、4箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/components/board-project-detail/board-project-detail-content.tsx` | 188 | Link href |
| `src/components/run-detail/run-detail-content.tsx` | 85 | breadcrumb |
| `src/components/checks/findings-content.tsx` | 90 | breadcrumb |
| `src/components/diff/diff-content.tsx` | 51 | breadcrumb |

#### 7. `` `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}` ``（動的、9箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/components/runs/runs-list-content.tsx` | 74 | Link href |
| `src/components/board-project-detail/board-project-detail-content.tsx` | 90, 132 | Link href |
| `src/components/run-detail/run-detail-content.tsx` | 254 | Link href |
| `src/components/diff/diff-content.tsx` | 54, 75, 86, 435, 437 | breadcrumb, Link href, 変数 |
| `src/components/checks/findings-content.tsx` | 93 | breadcrumb |

#### 8. `` `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${checkKind}` ``（動的、2箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/components/run-detail/run-detail-content.tsx` | 145 | Link href |
| `src/components/checks/findings-content.tsx` | 74 | basePath変数 |

#### 9. `` `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/diff` ``（動的、1箇所）

| ファイル | 行 | 用途 |
|---|---|---|
| `src/components/run-detail/run-detail-content.tsx` | 328 | Link href |

### 重複が顕著なパターン

特に **breadcrumb 構築** が最大の重複源。以下の5ファイルがほぼ同一のbreadcrumb階層を構築している:

1. `runs/runs-list-content.tsx` — Repositories > Repo > Board > Runs
2. `run-detail/run-detail-content.tsx` — Repositories > Repo > Board > Runs > RunId
3. `checks/findings-content.tsx` — Repositories > Repo > Board > Runs > RunId > Findings
4. `diff/diff-content.tsx` — Repositories > Repo > Board > Runs > RunId > Diff
5. `board-project-detail/board-project-detail-content.tsx` — Repositories > Repo

各breadcrumbの前半部分（Repositories → Repo → Board → Runs）は共通パターンで、ルート関数の集約だけでなく **breadcrumb builder の共通化** も合わせて検討する価値がある。ただし、breadcrumb共通化はIssue #108のスコープ外の可能性があるため、routes.ts への集約を先行し、breadcrumb共通化は後続Issueとして提案すべき。

### 対象外（外部URL・動的データURL）

以下はルート集約の対象外:
- `artifact-viewer/download-list.tsx:29` — `d.url`（APIから取得したダウンロードURL）
- `artifact-viewer/svg-viewer.tsx:41` — `tab.source.url`（外部URL）
- `artifact-viewer/pdf-viewer.tsx:19` — `primary.url`（外部URL）
- `artifact-viewer/artifact-viewer-section.tsx:228` — `viewer.primary.url`（外部URL）
- `repository-detail/repository-detail-content.tsx:53` — `repo.html_url`（GitHub URL）
- `board-project-detail/board-project-detail-content.tsx:79` — `project.issue_url`（GitHub URL）

### 提案する routes.ts の関数シグネチャ

```typescript
// boardflow/src/lib/routes.ts
export const routes = {
  login: () => '/login',
  repositories: () => '/repositories',
  repository: (repositoryId: string) => `/repositories/${repositoryId}`,
  repositoryTokens: (repositoryId: string) => `/repositories/${repositoryId}/settings/tokens`,
  board: (repositoryId: string, boardProjectId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}`,
  runs: (repositoryId: string, boardProjectId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs`,
  run: (repositoryId: string, boardProjectId: string, boardRunId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`,
  runChecks: (repositoryId: string, boardProjectId: string, boardRunId: string, checkKind: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${checkKind}`,
  runDiff: (repositoryId: string, boardProjectId: string, boardRunId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/diff`,
} as const;
```

### 影響範囲（変更対象ファイル）

| ファイル | 変更箇所数 |
|---|---|
| `src/lib/routes.ts` | **新規作成** |
| `src/middleware.ts` | 3 |
| `src/app/(authenticated)/layout.tsx` | 1 |
| `src/app/login/page.tsx` | 1 |
| `src/components/layout/header.tsx` | 1 |
| `src/components/layout/sidebar.tsx` | 1 |
| `src/components/repositories/repositories-list.tsx` | 1 |
| `src/components/repository-detail/repository-detail-content.tsx` | 3 |
| `src/components/board-project-detail/board-project-detail-content.tsx` | 6 |
| `src/components/runs/runs-list-content.tsx` | 4 |
| `src/components/run-detail/run-detail-content.tsx` | 7 |
| `src/components/checks/findings-content.tsx` | 6 |
| `src/components/diff/diff-content.tsx` | 9 |
| `src/components/tokens/tokens-page-content.tsx` | 2 |
| **合計** | **1新規 + 13既存ファイル、約45箇所** |

## 結論ステータス

**`implementation_required`**

外部ライブラリ調査は不要。純粋なコードリファクタリングとして実装に進むべき。

## 残リスク

- breadcrumb構築パターンの重複は routes.ts 集約だけでは完全に解消されない。breadcrumb builder の共通化は後続Issueとして提案可能。
- `middleware.ts` はサーバーサイドで実行されるため、routes.ts のインポートが正常に動作するか確認が必要（通常のTypeScriptモジュールなので問題ないはず）。
- `header.tsx` の `window.location.href = '/login'` は `routes.login()` に置換可能だが、クライアントサイドでの直接遷移であり、Next.jsの `router.push` ではない点に注意。

## 参照URL

- Issue: https://github.com/f0reachARR/boardflow/issues/108

---

## 計画フェーズ（2026-05-14）

### 目的

`boardflow/src/lib/routes.ts` にルート生成関数を集約し、フロントエンド全体で約46箇所にハードコードされたルート文字列を関数呼び出しに置き換える。URL構造変更時の修正箇所を1ファイルに集約する。

### 非目的

- ルート構造（URLパス）そのものの変更
- breadcrumb builder の共通化（後続Issue候補）
- API エンドポイントURL（`/api/v1/...`）の集約
- 外部URL（GitHub URL、ダウンロードURLなど）の集約
- 挙動の変更

### 受け入れ条件

1. `boardflow/src/lib/routes.ts` が新規作成され、9つのルート生成関数がexportされている
2. 既存13ファイルのハードコードされたルート文字列が `routes.*()` 呼び出しに置き換えられている
3. `pnpm typecheck` が通る
4. `pnpm lint` が通る
5. `pnpm build` が通る
6. 画面遷移の挙動が変わらないこと（生成されるURL文字列が完全に一致すること）

### 詳細要件

#### 新規ファイル: `boardflow/src/lib/routes.ts`

```typescript
export const routes = {
  login: () => '/login' as const,
  repositories: () => '/repositories' as const,
  repository: (repositoryId: string | number) =>
    `/repositories/${repositoryId}` as const,
  repositoryTokens: (repositoryId: string | number) =>
    `/repositories/${repositoryId}/settings/tokens` as const,
  board: (repositoryId: string | number, boardProjectId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}` as const,
  runs: (repositoryId: string | number, boardProjectId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs` as const,
  run: (repositoryId: string | number, boardProjectId: string, boardRunId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}` as const,
  runChecks: (repositoryId: string | number, boardProjectId: string, boardRunId: string, checkKind: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${checkKind}` as const,
  runDiff: (repositoryId: string | number, boardProjectId: string, boardRunId: string) =>
    `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/diff` as const,
};
```

**設計判断:**
- `repositoryId` は `string | number` を受け入れる（APIレスポンスでは number、URL params では string のため）
- `boardProjectId`, `boardRunId`, `checkKind` は常に string
- `as const` で戻り値型をリテラルテンプレートに推論させる
- オブジェクトに集約（名前空間として `routes.xxx()` で呼び出し）
- named export（`export const routes`）

#### 既存ファイル変更一覧

**ファイル1: `src/middleware.ts`（4箇所）**
| 行 | 現在のコード | 変更後 |
|---|---|---|
| 4 | `const PUBLIC_PATHS = ['/login'];` | `const PUBLIC_PATHS = [routes.login()];` |
| 12 | `new URL('/repositories', request.url)` | `new URL(routes.repositories(), request.url)` |
| 14 | `new URL('/login', request.url)` | `new URL(routes.login(), request.url)` |
| 25 | `new URL('/login', request.url)` | `new URL(routes.login(), request.url)` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル2: `src/app/login/page.tsx`（1箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 14 | `redirect('/repositories');` | `redirect(routes.repositories());` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル3: `src/app/(authenticated)/layout.tsx`（1箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 9 | `redirect('/login');` | `redirect(routes.login());` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル4: `src/components/layout/header.tsx`（1箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 55 | `window.location.href = '/login';` | `window.location.href = routes.login();` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル5: `src/components/layout/sidebar.tsx`（1箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 8 | `{ href: '/repositories', label: ... }` | `{ href: routes.repositories(), label: ... }` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル6: `src/components/repositories/repositories-list.tsx`（1箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 49 | `` href={`/repositories/${repo.github_repository_id}`} `` | `href={routes.repository(repo.github_repository_id)}` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル7: `src/components/repository-detail/repository-detail-content.tsx`（3箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 38 | `{ label: 'Repositories', href: '/repositories' }` | `{ label: 'Repositories', href: routes.repositories() }` |
| 66 | `` href={`/repositories/${repositoryId}/settings/tokens`} `` | `href={routes.repositoryTokens(repositoryId)}` |
| 96 | `` href={`/repositories/${repositoryId}/boards/${project.board_project_id}`} `` | `href={routes.board(repositoryId, project.board_project_id)}` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル8: `src/components/board-project-detail/board-project-detail-content.tsx`（6箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 39 | `{ label: 'Repositories', href: '/repositories' }` | `{ label: 'Repositories', href: routes.repositories() }` |
| 42 | `` href: `/repositories/${repositoryId}` `` | `href: routes.repository(repositoryId)` |
| 64 | `` href={`/repositories/${repositoryId}`} `` | `href={routes.repository(repositoryId)}` |
| 90 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${project.latest_completed_run_id}`} `` | `href={routes.run(repositoryId, boardProjectId, project.latest_completed_run_id)}` |
| 132 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${run.board_run_id}`} `` | `href={routes.run(repositoryId, boardProjectId, run.board_run_id)}` |
| 188 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs`} `` | `href={routes.runs(repositoryId, boardProjectId)}` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル9: `src/components/runs/runs-list-content.tsx`（4箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 39 | `{ label: 'Repositories', href: '/repositories' }` | `{ label: 'Repositories', href: routes.repositories() }` |
| 42 | `` href: `/repositories/${repositoryId}` `` | `href: routes.repository(repositoryId)` |
| 46 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}` `` | `href: routes.board(repositoryId, boardProjectId)` |
| 74 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${run.board_run_id}`} `` | `href={routes.run(repositoryId, boardProjectId, run.board_run_id)}` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル10: `src/components/run-detail/run-detail-content.tsx`（7箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 76 | `{ label: 'Repositories', href: '/repositories' }` | `{ label: 'Repositories', href: routes.repositories() }` |
| 79 | `` href: `/repositories/${repositoryId}` `` | `href: routes.repository(repositoryId)` |
| 83 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}` `` | `href: routes.board(repositoryId, boardProjectId)` |
| 85 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs` `` | `href: routes.runs(repositoryId, boardProjectId)` |
| 145 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${check.kind}`} `` | `href={routes.runChecks(repositoryId, boardProjectId, boardRunId, check.kind)}` |
| 254 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${diff.base_board_run_id}`} `` | `href={routes.run(repositoryId, boardProjectId, diff.base_board_run_id)}` |
| 328 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/diff`} `` | `href={routes.runDiff(repositoryId, boardProjectId, boardRunId)}` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル11: `src/components/checks/findings-content.tsx`（6箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 74 | `` const basePath = `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${checkKind}` `` | `const basePath = routes.runChecks(repositoryId, boardProjectId, boardRunId, checkKind)` |
| 81 | `{ label: 'Repositories', href: '/repositories' }` | `{ label: 'Repositories', href: routes.repositories() }` |
| 84 | `` href: `/repositories/${repositoryId}` `` | `href: routes.repository(repositoryId)` |
| 88 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}` `` | `href: routes.board(repositoryId, boardProjectId)` |
| 90 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs` `` | `href: routes.runs(repositoryId, boardProjectId)` |
| 93 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}` `` | `href: routes.run(repositoryId, boardProjectId, boardRunId)` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル12: `src/components/diff/diff-content.tsx`（9箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 42 | `{ label: 'Repositories', href: '/repositories' }` | `{ label: 'Repositories', href: routes.repositories() }` |
| 45 | `` href: `/repositories/${repositoryId}` `` | `href: routes.repository(repositoryId)` |
| 49 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}` `` | `href: routes.board(repositoryId, boardProjectId)` |
| 51 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs` `` | `href: routes.runs(repositoryId, boardProjectId)` |
| 54 | `` href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}` `` | `href: routes.run(repositoryId, boardProjectId, boardRunId)` |
| 75 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${diff.base_board_run_id}`} `` | `href={routes.run(repositoryId, boardProjectId, diff.base_board_run_id)}` |
| 86 | `` href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`} `` | `href={routes.run(repositoryId, boardProjectId, boardRunId)}` |
| 435 | `` const currentRunUrl = `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}` `` | `const currentRunUrl = routes.run(repositoryId, boardProjectId, boardRunId)` |
| 437-438 | `` `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${baseRunId}` `` | `routes.run(repositoryId, boardProjectId, baseRunId)` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

**ファイル13: `src/components/tokens/tokens-page-content.tsx`（2箇所）**
| 行 | 現在 | 変更後 |
|---|---|---|
| 25 | `{ label: 'Repositories', href: '/repositories' }` | `{ label: 'Repositories', href: routes.repositories() }` |
| 26 | `` href: `/repositories/${repositoryId}` `` | `href: routes.repository(repositoryId)` |
| + | import追加 | `import { routes } from '@/lib/routes';` |

### 影響範囲

- フロントエンド（`boardflow/`）のみ
- バックエンド（`crates/`）: 影響なし
- DBマイグレーション: 影響なし
- OpenAPI スキーマ: 影響なし
- CI: フロントエンドの typecheck / lint / build ステップのみ

### 設計方針

1. **純粋なコード移動**: テンプレートリテラルを関数呼び出しに置き換えるだけ。生成されるURL文字列は完全に同一
2. **オブジェクト集約**: `routes.xxx()` 形式でIDEのオートコンプリートが効く
3. **型安全**: `repositoryId` に `string | number` を受け入れ、呼び出し側でのキャスト不要
4. **as const**: テンプレートリテラル型を保持し、型推論を最大化

### テスト観点

1. **静的検証**:
   - `pnpm typecheck` — 型エラーなし
   - `pnpm lint` — Biome lint エラーなし
   - `pnpm build` — ビルド成功
2. **回帰テスト**:
   - 各ルート関数が期待するURL文字列を生成することを手動確認（生成コードはテンプレートリテラルと同一なので自明）
   - ユニットテストの追加は任意（純粋な文字列結合のため、型検査で十分）
3. **動作確認**:
   - 主要画面の遷移が正常に動作すること（ブラウザテスト、スコープ外だが推奨）

### ドキュメント更新対象

- `docs/logs/108/worklog.md` — 本計画と実装結果を記録（対応済み）
- その他のドキュメント更新は不要（コード内部リファクタリングのため）

### 実装順序

1. **Step 0**: `feature/issue-108-route-helpers` ブランチを作成
2. **Step 1**: `boardflow/src/lib/routes.ts` を新規作成
3. **Step 2**: 静的ルートのみ使用するファイルを先に変更（依存が少ない）
   - `src/middleware.ts`
   - `src/app/login/page.tsx`
   - `src/app/(authenticated)/layout.tsx`
   - `src/components/layout/header.tsx`
   - `src/components/layout/sidebar.tsx`
4. **Step 3**: 1引数（repositoryId のみ）を使うファイルを変更
   - `src/components/repositories/repositories-list.tsx`
   - `src/components/tokens/tokens-page-content.tsx`
5. **Step 4**: 2-3引数を使うファイルを変更
   - `src/components/repository-detail/repository-detail-content.tsx`
   - `src/components/board-project-detail/board-project-detail-content.tsx`
   - `src/components/runs/runs-list-content.tsx`
6. **Step 5**: 全ルート関数を使うファイルを変更
   - `src/components/run-detail/run-detail-content.tsx`
   - `src/components/checks/findings-content.tsx`
   - `src/components/diff/diff-content.tsx`
7. **Step 6**: `pnpm typecheck && pnpm lint && pnpm build` で検証
8. **Step 7**: コミット・PR作成

### 実装要否

**`implementation_required`**

### 未解決の疑問

なし。Issue本文、調査結果、コード確認から十分な情報が揃っており、仕様に曖昧な点はない。

### 残リスク

- breadcrumb構築パターンの重複は routes.ts 集約だけでは解消されない（後続Issue候補として提案可能）
- `board-project-detail-content.tsx` L90 の `project.latest_completed_run_id` は nullable（`string | null`）。`routes.run()` の `boardRunId` 引数は `string` のため、呼び出し元で null チェックが既にあることを確認する必要がある → 確認済み: L89 に `{project.latest_completed_run_id && (` の条件分岐あり、問題なし
- `diff-content.tsx` L437-438 の `baseRunId` も nullable。L436 に `baseRunId ?` の三項演算子あり、問題なし

---

## 実装フェーズ（2026-05-14）

### 実行内容

1. `feature/issue-108-route-helpers` ブランチをmainから作成
2. `boardflow/src/lib/routes.ts` を新規作成（9関数、`as const` なし・シンプル版で実装）
3. 13ファイル・約46箇所のハードコードされたルート文字列を `routes.*()` 呼び出しに置き換え
4. Biome自動フォーマットを適用（7ファイルのフォーマット修正）
5. コミット: `feat: extract route helpers into routes.ts (#108)`

### テスト結果

| チェック | 結果 |
|---|---|
| `pnpm typecheck` | ✅ パス |
| `pnpm lint` | ✅ パス |
| `pnpm build` | ✅ パス（全11ルート正常ビルド） |

### 変更ファイル一覧（14ファイル）

- **新規**: `boardflow/src/lib/routes.ts`
- **変更**: middleware.ts, login/page.tsx, (authenticated)/layout.tsx, header.tsx, sidebar.tsx, repositories-list.tsx, tokens-page-content.tsx, repository-detail-content.tsx, board-project-detail-content.tsx, runs-list-content.tsx, run-detail-content.tsx, findings-content.tsx, diff-content.tsx

### 設計判断の差分

- 計画の `as const` は省略（Biomeフォーマットとの相性・可読性を考慮。関数の戻り値型はテンプレートリテラルで自動推論されるため問題なし）

### 発見した問題点

- Biomeフォーマッターが `<Link href={routes.xxx(...)}>` を1行に収めるケースがあった（元のテンプレートリテラルでは複数行だった箇所）。自動フォーマットで解決。
- `findings-content.tsx` のimport順序がBiomeのorganizeImportsルールに違反。自動修正で解決。

### 残リスク

- なし。純粋なコード移動・抽出のみで挙動変更なし。
- breadcrumb共通化は後続Issue候補。
