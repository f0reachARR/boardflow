# Issue #107 — 共通フォーマット処理の集約

## Issueまでの経緯

フロントエンド (`boardflow/src/`) の複数コンポーネントに、ステータス色判定・日時フォーマット・短縮ID・バイトサイズ変換などのヘルパー関数がコピー＆ペーストで散在している。保守性向上のため、これらを `src/lib/domain/` や `src/lib/format.ts` に集約する。

## ユーザー要望

- `src/lib/domain/status.ts` などにステータス色の判定を集約する
- `src/lib/format.ts` などに日時、短縮ID、バイト数の表示処理を集約する
- 既存コンポーネントから重複した helper を削除する
- 既存の表示仕様は原則変えない

## 調査結果

### 1. ステータス色の散在状況

6ファイルに7つのステータス色関数が定義されている。

| ファイル | 関数名 | 対象ステータス | カラーマッピング |
|---|---|---|---|
| `run-detail/run-detail-content.tsx:10` | `statusColor(status: string)` | BoardRunStatus | completed→green, failed→red, timed_out→orange, default→gray |
| `run-detail/run-detail-content.tsx:23` | `checkStatusColor(status: string)` | RunCheckStatus | passed→green, failed→red, default→gray |
| `run-detail/run-detail-content.tsx:34` | `artifactStatusColor(status: string)` | ArtifactStatus | available→green, missing→orange, failed→red, skipped→gray, default→gray |
| `repositories/repositories-list.tsx:7` | `statusColor(status: string \| null)` | BoardRunStatus (nullable) | completed→green, failed→red, timed_out→orange, processing/importing→blue, default→gray |
| `runs/runs-list-content.tsx:8` | `statusColor(status: string)` | BoardRunStatus | completed→green, failed→red, timed_out→orange, created/uploading/importing→blue, default→gray |
| `board-project-detail/board-project-detail-content.tsx:8` | `statusColor(status: string)` | BoardRunStatus | completed→green, failed→red, timed_out→orange, created/uploading/importing→blue, default→gray |
| `diff/diff-content.tsx:9` | `diffStatusColor(status: string)` | BoardRunDiffStatus | ready→green, no_baseline→gray, unavailable→orange, failed→red, default→gray |
| `repository-detail/repository-detail-content.tsx:9` | `stateColor(state: string)` | project state | completed→green, failed→red, timed_out→orange, processing→blue, detected→gray, default→gray |

**注意**: `run-detail` の `statusColor` は in-progress 系ステータス (created/uploading/importing) を gray にフォールバックしている。他の `statusColor` (`runs-list-content`, `board-project-detail`) では blue にマップしている。`repositories-list` では `processing/importing` を blue にマップ。集約時にこの差異を意識する必要がある。

追加の重複ヘルパー:
- `checkBadge` 関数: `runs-list-content.tsx:25` と `board-project-detail-content.tsx:25` に同一実装
- `isRecord` 関数: `run-detail-content.tsx:49` と `diff-content.tsx:24` に同一実装

### 2. 日時フォーマット処理の散在状況

| ファイル | 行 | 処理 |
|---|---|---|
| `runs/runs-list-content.tsx` | L148 | `new Date(run.created_at).toLocaleString()` |
| `repositories/repositories-list.tsx` | L87 | `new Date(repo.updated_at).toLocaleDateString()` |
| `tokens/token-list.tsx` | L59 | `new Date(token.created_at).toLocaleDateString()` |
| `tokens/token-list.tsx` | L64 | `new Date(token.last_used_at).toLocaleDateString()` (nullable) |
| `run-detail/run-detail-content.tsx` | L177 | `new Date(run.created_at).toLocaleString()` |
| `run-detail/run-detail-content.tsx` | L179 | `new Date(run.completed_at).toLocaleString()` |
| `repository-detail/repository-detail-content.tsx` | L66 | `new Date(repo.created_at).toLocaleDateString()` |
| `repository-detail/repository-detail-content.tsx` | L144 | `new Date(project.updated_at).toLocaleDateString()` |
| `diff/diff-content.tsx` | L144 | `new Date(diff.created_at).toLocaleString()` |
| `board-project-detail/board-project-detail-content.tsx` | L130 | `new Date(project.created_at).toLocaleString()` |
| `board-project-detail/board-project-detail-content.tsx` | L206 | `new Date(run.created_at).toLocaleString()` |

2パターン:
- `toLocaleString()` — 日時表示 (7箇所)
- `toLocaleDateString()` — 日付のみ表示 (4箇所)

### 3. 短縮ID処理の散在状況

| ファイル | 行 | 処理 | 文字数 |
|---|---|---|---|
| `board-project-detail/board-project-detail-content.tsx` | L168 | `run.commit_sha.slice(0, 7)` | 7 |
| `checks/findings-content.tsx` | L91 | `boardRunId.slice(0, 8)` | 8 |
| `run-detail/run-detail-content.tsx` | L161 | `boardRunId.slice(0, 8)` | 8 |
| `run-detail/run-detail-content.tsx` | L175 | `run.commit_sha.slice(0, 7)` | 7 |
| `run-detail/run-detail-content.tsx` | L340 | `diff.base_board_run_id.slice(0, 8)` | 8 |
| `runs/runs-list-content.tsx` | L110 | `run.commit_sha.slice(0, 7)` | 7 |
| `diff/diff-content.tsx` | L104 | `boardRunId.slice(0, 8)` | 8 |
| `diff/diff-content.tsx` | L129 | `diff.base_board_run_id.slice(0, 8)` | 8 |
| `diff/diff-content.tsx` | L140 | `boardRunId.slice(0, 8)` | 8 |
| `diff/diff-content.tsx` | L503 | `boardRunId.slice(0, 8)` | 8 |
| `diff/diff-content.tsx` | L512 | `baseRunId.slice(0, 8)` | 8 |

2パターン:
- commit SHA → 7文字 (3箇所)
- board_run_id / UUID → 8文字 (8箇所)

### 4. バイトサイズ表示処理の散在状況

| ファイル | 行 | 処理 |
|---|---|---|
| `run-detail/run-detail-content.tsx` | L292-293 | `artifact.size_bytes ? \`${(artifact.size_bytes / 1024).toFixed(1)} KB\` : '—'` |

現時点では1箇所のみ。ただし今後アーティファクト表示が増える可能性があるため、集約対象として妥当。

### 5. 既存の lib/domain/ や lib/format.ts の状態

- `boardflow/src/lib/domain/` — **存在しない** (新規作成が必要)
- `boardflow/src/lib/format.ts` — **存在しない** (新規作成が必要)
- `boardflow/src/lib/` には現在 `api/`, `auth.ts`, `query-client.ts` のみ存在

### 6. 使用されているステータスの型定義

すべて `boardflow/src/lib/api/schema.d.ts` (自動生成) で定義:

| 型名 | 値 | 使用箇所 |
|---|---|---|
| `BoardRunStatus` (L512) | `'created' \| 'uploading' \| 'importing' \| 'completed' \| 'failed' \| 'timed_out'` | run-detail, runs-list, board-project-detail, repositories-list |
| `CreateBoardRunStatus` (L558) | `'created' \| 'importing' \| 'completed' \| 'failed' \| 'timed_out'` | API レスポンス |
| `RunCheckStatus` (L799) | `'passed' \| 'failed' \| 'skipped'` | run-detail (checkStatusColor) |
| `ArtifactStatus` (L399) | `'available' \| 'missing' \| 'failed' \| 'skipped'` | run-detail (artifactStatusColor) |
| `BoardRunDiffStatus` (L486) | `'ready' \| 'no_baseline' \| 'unavailable' \| 'failed'` | diff-content (diffStatusColor) |
| `CheckStatus` (L527) | `'passed' \| 'failed' \| 'skipped'` | ≒ RunCheckStatus |

`repository-detail-content.tsx` の `stateColor` は project の `state` フィールドに使用されるが、スキーマ上で明示的な enum 型は見当たらない（`detected`, `processing`, `completed`, `failed`, `timed_out` を想定）。

## 計画（実装エージェント向けの推奨）

### 新規ファイル

1. **`src/lib/domain/status.ts`** — ステータス色関数を集約
   - `boardRunStatusColor(status: string | null): string`
   - `checkStatusColor(status: string): string`
   - `artifactStatusColor(status: string): string`
   - `diffStatusColor(status: string): string`
   - `projectStateColor(state: string): string`

2. **`src/lib/format.ts`** — フォーマット関数を集約
   - `formatDateTime(iso: string): string` → `toLocaleString()`
   - `formatDate(iso: string): string` → `toLocaleDateString()`
   - `shortSha(sha: string): string` → `.slice(0, 7)`
   - `shortId(id: string): string` → `.slice(0, 8)`
   - `formatBytes(bytes: number | null | undefined): string` → KB 変換

3. **`src/lib/domain/guards.ts`** (任意) — `isRecord` などの型ガードを集約

### 注意点

- `run-detail` の `statusColor` は `created/uploading/importing` を gray フォールバックにしている点が他と異なる。集約時は `runs-list-content` / `board-project-detail` のマッピング (blue) を採用し、`run-detail` の挙動を揃えるか、ユーザー確認が必要。
- `checkBadge` は色判定＋JSXレンダリングが一体なので、色判定部分のみ `checkStatusColor` に委譲し、JSX部分はコンポーネント内に残すのが自然。
- `schema.d.ts` は自動生成ファイルなので編集不可。型を import して使う。

## 実装

### 新規作成ファイル (4件)

1. **`boardflow/src/lib/domain/status.ts`** — ステータス色関数6つ
   - `boardRunStatusColor(status: BoardRunStatus | string | null): string` — created/uploading/importing/processing → blue, completed → green, failed → red, timed_out → orange
   - `checkStatusColor(status: RunCheckStatus | string): string` — passed → green, failed → red, default → gray
   - `artifactStatusColor(status: ArtifactStatus | string): string` — available → green, missing → orange, failed → red, skipped → gray
   - `diffStatusColor(status: BoardRunDiffStatus | string): string` — ready → green, no_baseline → gray, unavailable → orange, failed → red
   - `projectStateColor(state: BoardProjectState | string): string` — completed → green, processing → blue, detected → gray, etc.
   - `checkBadgeColor(status: RunCheckStatus | string): string` — Chakra colorPalette用の solid variant

2. **`boardflow/src/lib/domain/guards.ts`** — 型ガード関数5つ
   - `isRecord`, `isFileChanges`, `isBomChanges`, `isCheckEntry`, `isArtifactChanges`

3. **`boardflow/src/lib/format.ts`** — フォーマット関数5つ
   - `formatDateTime(date: string | Date): string` — toLocaleString()
   - `formatDate(date: string | Date): string` — toLocaleDateString()
   - `shortSha(sha: string): string` — slice(0, 7)
   - `shortId(id: string): string` — slice(0, 8)
   - `formatBytes(bytes: number): string` — KB/MB/GB変換

4. **`boardflow/src/components/ui/check-badge.tsx`** — CheckBadgeコンポーネント
   - `runs-list-content.tsx` と `board-project-detail-content.tsx` の重複 checkBadge 関数をReactコンポーネントに

### 修正ファイル (9件)

1. **`boardflow/src/lib/api/schema-types.ts`** — `ArtifactStatus`, `BoardProjectState`, `BoardRunDiffStatus`, `BoardRunStatus`, `RunCheckStatus` 型エイリアス追加
2. **`boardflow/src/components/run-detail/run-detail-content.tsx`** — ローカル statusColor, checkStatusColor, artifactStatusColor, isRecord, 型ガード群 (5関数) 削除 → 共通import
3. **`boardflow/src/components/repositories/repositories-list.tsx`** — ローカル statusColor 削除 → boardRunStatusColor + formatDate import
4. **`boardflow/src/components/runs/runs-list-content.tsx`** — ローカル statusColor, checkBadge 削除 → boardRunStatusColor, CheckBadge, shortSha, formatDateTime import
5. **`boardflow/src/components/board-project-detail/board-project-detail-content.tsx`** — 同上
6. **`boardflow/src/components/diff/diff-content.tsx`** — ローカル diffStatusColor, isRecord, 型ガード群 (5関数) 削除 → 共通import + shortId, formatDateTime
7. **`boardflow/src/components/repository-detail/repository-detail-content.tsx`** — ローカル stateColor 削除 → projectStateColor + formatDate import
8. **`boardflow/src/components/checks/findings-content.tsx`** — shortId import追加
9. **`boardflow/src/components/tokens/token-list.tsx`** — formatDate import追加

### 挙動変更

- `run-detail-content.tsx` の `statusColor` で in-progress系ステータス (created/uploading/importing) が **gray → blue** に統一（他コンポーネントと同じマッピング）

## テスト結果

- `pnpm lint`: OK (Biome check — 69 files, 0 errors)
- `pnpm typecheck`: OK (tsc --noEmit)
- `pnpm build`: OK (Next.js 16.2.4 Turbopack — 全ルート正常生成)

## 残リスク

- `formatDateTime`/`formatDate` はランタイムのロケールに依存。SSRとクライアントでロケールが異なる場合にハイドレーションミスマッチが発生しうる（既存の挙動と同一で新たなリスクではない）
- `checkBadgeColor` は今回のリファクタリングでは使用箇所なし（将来用に定義）
- `repository-detail-content.tsx` の `stateColor` は他のステータス色とは概念が異なる（project state vs. run status）。別関数として維持が妥当。

## ユーザー回答済みの方針

- **statusColor差異**: `run-detail-content.tsx` の in-progress系ステータスの色を gray → **blue に統一**。他ファイルの blue 側に合わせる。

## 計画フェーズ（2026-05-14）

### 結論ステータス

**`implementation_required`** — 外部ライブラリの調査は不要。純粋なコード移動・リファクタリングであり、実装に進むべき。

### ブランチ名

`refactor/issue-107-consolidate-format-helpers`

---

## 詳細実装計画

### 目的

- 6ファイル7関数に散在するステータス色判定を `src/lib/domain/status.ts` に集約
- 11箇所の日時フォーマット・11箇所の短縮ID・1箇所のバイトサイズ表示を `src/lib/format.ts` に集約
- 2ファイルに重複する `isRecord` 型ガードを `src/lib/domain/guards.ts` に集約
- 2ファイルに重複する `checkBadge` JSXヘルパーを1箇所に集約

### 非目的

- 新機能の追加
- コンポーネントのリファクタリング（ヘルパー集約以外）
- API型定義 (`schema.d.ts`) の変更
- テストの追加（既存テストなし、今回のスコープ外）

### 受け入れ条件

- `pnpm lint` がパスする
- `pnpm typecheck` がパスする
- `pnpm build` がパスする
- 表示上の挙動変更は `run-detail-content.tsx` の statusColor gray→blue 統一のみ
- 各コンポーネントからローカルヘルパー定義が削除され、import に置き換わっている

---

### Step 1: 新規ファイル `boardflow/src/lib/domain/status.ts` を作成

ステータス色判定関数を集約する。引数は `string` (nullable は `string | null`) を維持し、OpenAPI 型を import しない（既存コードが `string` で呼んでいるため）。

```ts
// boardflow/src/lib/domain/status.ts

/**
 * BoardRunStatus → colorPalette
 * in-progress系 (created, uploading, importing) は blue に統一
 */
export function boardRunStatusColor(status: string | null): string {
  switch (status) {
    case 'completed':
      return 'green';
    case 'failed':
      return 'red';
    case 'timed_out':
      return 'orange';
    case 'created':
    case 'uploading':
    case 'importing':
      return 'blue';
    default:
      return 'gray';
  }
}

/** RunCheckStatus → colorPalette */
export function checkStatusColor(status: string): string {
  switch (status) {
    case 'passed':
      return 'green';
    case 'failed':
      return 'red';
    default:
      return 'gray';
  }
}

/** ArtifactStatus → colorPalette */
export function artifactStatusColor(status: string): string {
  switch (status) {
    case 'available':
      return 'green';
    case 'missing':
      return 'orange';
    case 'failed':
      return 'red';
    case 'skipped':
      return 'gray';
    default:
      return 'gray';
  }
}

/** BoardRunDiffStatus → colorPalette */
export function diffStatusColor(status: string): string {
  switch (status) {
    case 'ready':
      return 'green';
    case 'no_baseline':
      return 'gray';
    case 'unavailable':
      return 'orange';
    case 'failed':
      return 'red';
    default:
      return 'gray';
  }
}

/** BoardProjectState → colorPalette */
export function projectStateColor(state: string): string {
  switch (state) {
    case 'completed':
      return 'green';
    case 'failed':
      return 'red';
    case 'timed_out':
      return 'orange';
    case 'processing':
      return 'blue';
    case 'detected':
      return 'gray';
    default:
      return 'gray';
  }
}

/** CheckStatus (nullable) → colorPalette (checkBadge用) */
export function checkBadgeColor(status: string | null | undefined): string {
  if (!status) return 'gray';
  return status === 'passed' ? 'green' : status === 'failed' ? 'red' : 'gray';
}
```

#### 設計判断
- `boardRunStatusColor`: 4ファイルの `statusColor` を統合。`null` 許容 (`repositories-list.tsx` が nullable)。`run-detail-content.tsx` の gray フォールバックは blue に統一（ユーザー確認済み）。
- `checkBadgeColor`: `checkBadge` JSX 関数内の色判定ロジックだけを抽出。JSXレンダリングはコンポーネント側に残す。
- `projectStateColor`: `repository-detail-content.tsx` の `stateColor` を移動。名前を明確化。

### Step 2: 新規ファイル `boardflow/src/lib/domain/guards.ts` を作成

`run-detail-content.tsx` と `diff-content.tsx` に重複する型ガードを集約。

```ts
// boardflow/src/lib/domain/guards.ts

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
```

**注意**: `isFileChanges`, `isBomChanges`, `isCheckEntry`, `isArtifactChanges` は `run-detail-content.tsx` と `diff-content.tsx` の両方に同一実装が存在する。これらも `guards.ts` に移動する。

### Step 3: 新規ファイル `boardflow/src/lib/format.ts` を作成

```ts
// boardflow/src/lib/format.ts

/** ISO 8601 日時文字列 → toLocaleString() */
export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString();
}

/** ISO 8601 日時文字列 → toLocaleDateString() */
export function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString();
}

/** commit SHA の短縮表示 (7文字) */
export function shortSha(sha: string): string {
  return sha.slice(0, 7);
}

/** UUID / board_run_id の短縮表示 (8文字) */
export function shortId(id: string): string {
  return id.slice(0, 8);
}

/** バイト数の表示 (KB単位、null/undefined は '—') */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return '—';
  return `${(bytes / 1024).toFixed(1)} KB`;
}
```

### Step 4: コンポーネント修正（ファイルごとの変更一覧）

#### 4-1. `run-detail/run-detail-content.tsx`

| 変更 | 内容 |
|---|---|
| **削除** | ローカル `statusColor` (L10-21), `checkStatusColor` (L23-31), `artifactStatusColor` (L34-46), `isRecord` (L49-51) |
| **import追加** | `boardRunStatusColor, checkStatusColor, artifactStatusColor` from `@/lib/domain/status` |
| **import追加** | `isRecord` from `@/lib/domain/guards` |
| **import追加** | `formatDateTime, shortSha, shortId, formatBytes` from `@/lib/format` |
| **呼び出し変更** | `statusColor(run.status)` → `boardRunStatusColor(run.status)` |
| **呼び出し変更** | `boardRunId.slice(0, 8)` → `shortId(boardRunId)` |
| **呼び出し変更** | `run.commit_sha.slice(0, 7)` → `shortSha(run.commit_sha)` |
| **呼び出し変更** | `new Date(run.created_at).toLocaleString()` → `formatDateTime(run.created_at)` |
| **呼び出し変更** | `new Date(run.completed_at).toLocaleString()` → `formatDateTime(run.completed_at)` |
| **呼び出し変更** | `artifact.size_bytes ? ...` → `formatBytes(artifact.size_bytes)` |
| **呼び出し変更** | `diff.base_board_run_id.slice(0, 8)` → `shortId(diff.base_board_run_id)` |
| **挙動変更** | statusColor: in-progress系が gray → blue に変更 |

`isFileChanges`, `isBomChanges`, `isCheckEntry`, `isArtifactChanges` は diff-content にも同一実装が存在するため `guards.ts` に移動するが、これらは run-detail-content 固有の使用もあるため、run-detail-content.tsx と diff-content.tsx の両方から利用されるか再確認が必要。 → 調査済み: 両ファイルで同一実装。`guards.ts` に移動する。

#### 4-2. `repositories/repositories-list.tsx`

| 変更 | 内容 |
|---|---|
| **削除** | ローカル `statusColor` (L7-21) |
| **import追加** | `boardRunStatusColor` from `@/lib/domain/status` |
| **import追加** | `formatDate` from `@/lib/format` |
| **呼び出し変更** | `statusColor(repo.latest_run_status)` → `boardRunStatusColor(repo.latest_run_status)` |
| **呼び出し変更** | `new Date(repo.updated_at).toLocaleDateString()` → `formatDate(repo.updated_at)` |

#### 4-3. `runs/runs-list-content.tsx`

| 変更 | 内容 |
|---|---|
| **削除** | ローカル `statusColor` (L8-23), `checkBadge` (L25-37) |
| **import追加** | `boardRunStatusColor, checkBadgeColor` from `@/lib/domain/status` |
| **import追加** | `formatDateTime, shortSha` from `@/lib/format` |
| **呼び出し変更** | `statusColor(run.status)` → `boardRunStatusColor(run.status)` |
| **呼び出し変更** | `run.commit_sha.slice(0, 7)` → `shortSha(run.commit_sha)` |
| **呼び出し変更** | `new Date(run.created_at).toLocaleString()` → `formatDateTime(run.created_at)` |
| **checkBadge リファクタ** | ローカル定義を削除。`checkBadge` をインラインJSXに展開し、色判定のみ `checkBadgeColor` を使用。または `checkBadge` 関数を残してその中で `checkBadgeColor` を呼ぶ。 |

**checkBadge の扱い**: JSXを返す関数なので `status.ts` (純粋ロジック) には移動不可。選択肢:
1. `checkBadge` をコンポーネント (`src/components/ui/check-badge.tsx`) として抽出
2. 各ファイルのローカル `checkBadge` 内で `checkBadgeColor` を使う

→ **方針**: `checkBadge` は小さなJSX片（Reactコンポーネントとして抽出するほどではない）なので、各ファイルにローカル `checkBadge` を残しつつ、色判定だけ `checkBadgeColor` に委譲する。ただし2ファイルで同一実装なので、共通UIコンポーネント `src/components/ui/check-badge.tsx` に抽出する方が DRY。

→ **最終方針**: `src/components/ui/check-badge.tsx` として小さな共有コンポーネントを作成する。

#### 4-4. `board-project-detail/board-project-detail-content.tsx`

| 変更 | 内容 |
|---|---|
| **削除** | ローカル `statusColor` (L8-23), `checkBadge` (L25-37) |
| **import追加** | `boardRunStatusColor` from `@/lib/domain/status` |
| **import追加** | `CheckBadge` from `@/components/ui/check-badge` |
| **import追加** | `formatDateTime, shortSha` from `@/lib/format` |
| **呼び出し変更** | `statusColor(run.status)` → `boardRunStatusColor(run.status)` |
| **呼び出し変更** | `run.commit_sha.slice(0, 7)` → `shortSha(run.commit_sha)` |
| **呼び出し変更** | `new Date(project.created_at).toLocaleString()` → `formatDateTime(project.created_at)` |
| **呼び出し変更** | `new Date(run.created_at).toLocaleString()` → `formatDateTime(run.created_at)` |
| **呼び出し変更** | `checkBadge(...)` → `<CheckBadge status={...} />` |

#### 4-5. `diff/diff-content.tsx`

| 変更 | 内容 |
|---|---|
| **削除** | ローカル `diffStatusColor` (L9-22), `isRecord` (L24-26), `isFileChanges`, `isBomChanges`, `isCheckEntry`, `isArtifactChanges` |
| **import追加** | `diffStatusColor` from `@/lib/domain/status` |
| **import追加** | `isRecord, isFileChanges, isBomChanges, isCheckEntry, isArtifactChanges` from `@/lib/domain/guards` |
| **import追加** | `formatDateTime, shortId` from `@/lib/format` |
| **呼び出し変更** | `boardRunId.slice(0, 8)` → `shortId(boardRunId)` (複数箇所) |
| **呼び出し変更** | `diff.base_board_run_id.slice(0, 8)` → `shortId(diff.base_board_run_id)` |
| **呼び出し変更** | `baseRunId.slice(0, 8)` → `shortId(baseRunId)` |
| **呼び出し変更** | `new Date(diff.created_at).toLocaleString()` → `formatDateTime(diff.created_at)` |

#### 4-6. `repository-detail/repository-detail-content.tsx`

| 変更 | 内容 |
|---|---|
| **削除** | ローカル `stateColor` (L9-24) |
| **import追加** | `projectStateColor` from `@/lib/domain/status` |
| **import追加** | `formatDate` from `@/lib/format` |
| **呼び出し変更** | `stateColor(project.state)` → `projectStateColor(project.state)` |
| **呼び出し変更** | `new Date(repo.created_at).toLocaleDateString()` → `formatDate(repo.created_at)` |
| **呼び出し変更** | `new Date(project.updated_at).toLocaleDateString()` → `formatDate(project.updated_at)` |

#### 4-7. `checks/findings-content.tsx`

| 変更 | 内容 |
|---|---|
| **import追加** | `shortId` from `@/lib/format` |
| **呼び出し変更** | `boardRunId.slice(0, 8)` → `shortId(boardRunId)` |

`severityColor` はこのファイル固有（severity の色判定）なので移動しない。

#### 4-8. `tokens/token-list.tsx`

| 変更 | 内容 |
|---|---|
| **import追加** | `formatDate` from `@/lib/format` |
| **呼び出し変更** | `new Date(token.created_at).toLocaleDateString()` → `formatDate(token.created_at)` |
| **呼び出し変更** | `token.last_used_at ? new Date(token.last_used_at).toLocaleDateString() : '—'` → `token.last_used_at ? formatDate(token.last_used_at) : '—'` |

### Step 5: 共有UIコンポーネント `boardflow/src/components/ui/check-badge.tsx` を作成

```tsx
import { Badge, Text } from '@chakra-ui/react';
import { checkBadgeColor } from '@/lib/domain/status';

export function CheckBadge({ status }: { status: string | null | undefined }) {
  if (!status) {
    return (
      <Text color='gray.400' fontSize='sm'>
        —
      </Text>
    );
  }
  return (
    <Badge colorPalette={checkBadgeColor(status)} size='sm'>
      {status}
    </Badge>
  );
}
```

---

### 実装順序

1. `src/lib/domain/guards.ts` 作成 — 依存なし
2. `src/lib/domain/status.ts` 作成 — 依存なし
3. `src/lib/format.ts` 作成 — 依存なし
4. `src/components/ui/check-badge.tsx` 作成 — status.ts に依存
5. `run-detail-content.tsx` 修正
6. `repositories-list.tsx` 修正
7. `runs-list-content.tsx` 修正
8. `board-project-detail-content.tsx` 修正
9. `diff-content.tsx` 修正
10. `repository-detail-content.tsx` 修正
11. `findings-content.tsx` 修正
12. `token-list.tsx` 修正
13. `pnpm lint` → `pnpm typecheck` → `pnpm build` で検証

### 影響範囲

- **変更ファイル**: 8コンポーネントファイル
- **新規ファイル**: 4ファイル (`status.ts`, `guards.ts`, `format.ts`, `check-badge.tsx`)
- **挙動変更**: `run-detail-content.tsx` の in-progress系ステータス色が gray → blue に変更（ユーザー確認済み）
- **削除対象**: 各ファイルのローカルヘルパー関数（ステータス色、型ガード、checkBadge）

### テスト観点

- 既存のフロントエンドテストは存在しない
- 検証は `pnpm lint` (Biome), `pnpm typecheck` (tsc), `pnpm build` (Next.js) で行う
- ブラウザでの目視確認は実装エージェントのスコープ外だが、推奨

### ドキュメント更新対象

- このワークログ (`docs/logs/107/worklog.md`)
- `docs/frontend/summary.md` に `src/lib/domain/` と `src/lib/format.ts` の存在を追記（任意）

### 残リスク

- `statusColor` の gray→blue 統一により、`run-detail` 画面で in-progress 状態の Badge 色が変わる。意図通りだが UI 確認が推奨される。
- `isFileChanges` 等の型ガードは DiffSummary の形状に依存しており、バックエンド側のレスポンス変更時に `guards.ts` を更新する必要がある。
- `checkBadge` を共有コンポーネント化することで、将来的に片方だけの挙動を変えたい場合にコンポーネントの分岐が必要になる。現時点では同一仕様のため問題なし。
