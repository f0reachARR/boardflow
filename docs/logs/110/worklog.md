# Issue #110 — frontend: RunDetailContent をセクション単位に分割する

## Issueまでの経緯

- `RunDetailContent` (約350行) がデータ取得、breadcrumb、ヘッダー、checks、artifact summary、artifacts table、diff summary、viewer 接続を1ファイルで担っている
- #107 で status/format 関数を `@/lib/domain/status`, `@/lib/format` に集約済み
- #109 で DiffSummary パース処理を `@/lib/domain/diff-summary` に共通化済み
- これらの前提を踏まえ、表示セクションをコンポーネント分割するリファクタリング

## ユーザー要望

- `run-detail/` 配下にセクションコンポーネントを切り出す
  - RunHeader, RunChecksSection, ArtifactSummarySection, ArtifactsTable, RunDiffSummaryCard
- `RunDetailContent` はデータ取得とセクション配置を中心に薄くする
- 表示仕様は変えない（純粋なコード移動・分割）

## 調査結果 (リサーチフェーズ: 2026-05-14)

### 外部調査の必要性判断

**不要**。理由：

1. 新規外部ライブラリの導入なし — Chakra UI v3, Next.js, TanStack React Query はすべて既存導入済み
2. 外部API変更なし — バックエンドエンドポイントの変更は不要
3. 既存コンポーネントの移動・分割のみ — フレームワーク固有のパターン調査も不要（既にプロジェクト内で同様のパターンが確立されている）

### 現状のファイル構成

- `boardflow/src/components/run-detail/run-detail-content.tsx` (約350行、1ファイルのみ)
- データ取得: `useSuspenseQuery` × 4 + `useQuery` × 1 (diff)
- 表示セクション: Breadcrumb → Header → Checks → ArtifactSummary → ArtifactsTable → DiffSummary → ArtifactViewerSection

### 分割計画（実装エージェント向け）

| 新コンポーネント | 担当セクション | 主なprops |
|---|---|---|
| `RunHeader` | HStack(Heading + Badge) + メタ情報 | `run` |
| `RunChecksSection` | Checks カード群 | `checks`, `repositoryId`, `boardProjectId`, `boardRunId` |
| `ArtifactSummarySection` | Artifact Summary バッジ群 | `artifactSummary` |
| `ArtifactsTable` | Artifacts テーブル | `artifacts` |
| `RunDiffSummaryCard` | Changes from Baseline セクション全体 | `diff`, `diffErrorMessage`, `repositoryId`, `boardProjectId`, `boardRunId` |

- `RunDetailContent` はデータ取得 (5 queries) + Breadcrumb + 上記コンポーネントの配置のみに留める
- `ArtifactViewerSection` は既に別コンポーネントなのでそのまま

## 結論ステータス

**`implementation_required`** — 外部調査不要。実装エージェントへの引き渡しで完了。

## 計画 (planフェーズ: 2026-05-14)

### 目的

`RunDetailContent` (約290行) を5つのセクションコンポーネントに分割し、各セクションの責務を明確にする。`RunDetailContent` はデータ取得 + Breadcrumb + セクション配置のオーケストレーターに薄くする。

### 非目的

- 表示仕様の変更
- データ取得ロジックの変更（クエリは `RunDetailContent` に残す）
- `ArtifactViewerSection` の変更（既に分離済み）
- 新しい外部ライブラリの導入
- テストの追加（表示仕様が変わらないため）

### 受け入れ条件

1. `pnpm lint` がパスすること
2. `pnpm typecheck` がパスすること
3. `pnpm build` がパスすること
4. ブラウザでの表示が変わらないこと（目視確認）
5. `RunDetailContent` がデータ取得 + Breadcrumb + セクション配置のみになること

### 詳細要件

#### 1. 型エイリアスの追加 (`boardflow/src/lib/api/schema-types.ts`)

既存パターンに合わせ、セクションコンポーネントの Props で使う型エイリアスを3つ追加:

```ts
export type ArtifactSummary = components['schemas']['ArtifactSummary'];
export type BoardRunDetail = components['schemas']['BoardRunDetailResponse'];
export type CheckInfo = components['schemas']['CheckInfo'];
```

#### 2. 新規ファイル一覧とProps定義

すべて `boardflow/src/components/run-detail/` 配下にフラット配置。

##### (a) `run-header.tsx` — Run ID + ステータスバッジ + コミット/ブランチ/日時情報

```ts
interface RunHeaderProps {
  run: BoardRunDetail;
}
```

現在のコード L89–L102 を移動。import: `BoardRunDetail` from schema-types, `boardRunStatusColor` from status, `shortSha`, `formatDateTime` from format, Chakra UI コンポーネント。

##### (b) `run-checks-section.tsx` — Checks カード群

```ts
interface RunChecksSectionProps {
  checks: CheckInfo[];
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
}
```

現在のコード L105–L146 を移動。`checks.length === 0` のときは `null` を返す。import: `CheckInfo` from schema-types, `checkStatusColor` from status, `routes` from routes, Link, Chakra UI コンポーネント。

##### (c) `artifact-summary-section.tsx` — Artifact Summary バッジ群

```ts
interface ArtifactSummarySectionProps {
  artifactSummary: ArtifactSummary;
}
```

現在のコード L148–L163 を移動。import: `ArtifactSummary` from schema-types, Chakra UI コンポーネント。

##### (d) `artifacts-table.tsx` — Artifacts テーブル

```ts
interface ArtifactsTableProps {
  artifacts: Artifact[];
}
```

現在のコード L165–L200 を移動。`artifacts.length === 0` のときは `null` を返す。import: `Artifact` from schema-types, `artifactStatusColor` from status, `formatBytes` from format, Chakra UI コンポーネント。

##### (e) `run-diff-summary-card.tsx` — Changes from Baseline セクション全体

```ts
interface RunDiffSummaryCardProps {
  diff: DiffResponse | null;
  diffErrorMessage: string | null;
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
}
```

現在のコード L202–L284 を移動。`diff === null && diffErrorMessage === null` のときは `null` を返す。import: `DiffResponse` from schema-types, `parseDiffSummary` from diff-summary, `shortId` from format, `routes` from routes, Link, Chakra UI コンポーネント。

#### 3. `RunDetailContent` の変更

変更後の構造:

```tsx
export function RunDetailContent({ repositoryId, boardProjectId, boardRunId }: Props) {
  // データ取得 (5 queries) — 変更なし
  // derived data — 変更なし

  return (
    <Box>
      {project && <Breadcrumb items={[...]} />}
      <VStack align='stretch' gap={6}>
        <RunHeader run={run} />
        <RunChecksSection checks={run.checks} repositoryId={...} boardProjectId={...} boardRunId={...} />
        <ArtifactSummarySection artifactSummary={run.artifact_summary} />
        <ArtifactsTable artifacts={artifacts} />
        <RunDiffSummaryCard diff={diff} diffErrorMessage={diffErrorMessage} repositoryId={...} boardProjectId={...} boardRunId={...} />
        <ArtifactViewerSection viewers={viewers} expiresAt={viewerData?.expires_at} boardRunId={boardRunId} />
      </VStack>
    </Box>
  );
}
```

不要になった import を削除: `Table`, `Link` (Breadcrumb内では残る), status/format関数のうちセクションに移ったもの。ただし `shortId` は Breadcrumb で使っているため残る。

### 影響範囲

- **変更ファイル**: `run-detail-content.tsx`, `schema-types.ts`
- **新規ファイル**: 5ファイル (`run-header.tsx`, `run-checks-section.tsx`, `artifact-summary-section.tsx`, `artifacts-table.tsx`, `run-diff-summary-card.tsx`)
- **削除ファイル**: なし
- **他コンポーネントへの影響**: なし（`RunDetailContent` の外部インターフェース（Props）は変更なし）

### 設計方針

1. **条件付きレンダリングはセクション内部で行う**: `RunChecksSection` は `checks.length === 0` なら `null`、`ArtifactsTable` は `artifacts.length === 0` なら `null`、`RunDiffSummaryCard` は `diff === null && diffErrorMessage === null` なら `null` を返す。親の条件分岐を不要にして `RunDetailContent` を薄くする。
2. **`'use client'` ディレクティブは不要**: セクションコンポーネントは hooks を使わない（データは props 経由）。`RunDetailContent` のみ `'use client'` を持つ。
3. **export は named export**: 既存プロジェクトパターンに合わせる。
4. **ファイル命名はkebab-case**: 既存パターン (`run-detail-content.tsx`, `artifact-viewer-section.tsx`) に合わせる。

### テスト観点

- **既存テスト**: フロントエンドにはユニットテストフレームワークが導入されていないため、既存テストへの影響なし
- **新規テスト**: 表示仕様が変わらない純粋リファクタリングのため不要
- **検証方法**: `pnpm lint && pnpm typecheck && pnpm build` の3コマンドで静的検証

### ドキュメント更新対象

- なし（内部リファクタリングのため API/仕様ドキュメントの変更なし）

### 実装要否

**`implementation_required`**

### 未解決の疑問

- なし（純粋な内部リファクタリングのため仕様判断が不要）

### 検証手順

```bash
cd boardflow
pnpm lint
pnpm typecheck
pnpm build
```

### 実装順序

1. `schema-types.ts` に型エイリアス追加
2. 5つのセクションコンポーネントを新規作成（順序不問、並行可能）
3. `run-detail-content.tsx` を変更（セクションコンポーネントに切り替え、不要importを削除）
4. `pnpm lint && pnpm typecheck && pnpm build` で検証

## 残リスク

- なし（表示仕様変更なしの純粋リファクタリング）

## 実装 (implフェーズ: 2026-05-14)

### ブランチ
`refactor/run-detail-sections`

### 変更ファイル

#### 新規作成
- `boardflow/src/components/run-detail/run-header.tsx` — Run ヘッダー (ステータスバッジ, SHA, ブランチ, 日時)
- `boardflow/src/components/run-detail/run-checks-section.tsx` — Checks セクション (ERC/DRC カード)
- `boardflow/src/components/run-detail/artifact-summary-section.tsx` — Artifact Summary バッジ群
- `boardflow/src/components/run-detail/artifacts-table.tsx` — Artifacts テーブル
- `boardflow/src/components/run-detail/run-diff-summary-card.tsx` — Changes from Baseline カード

#### 変更
- `boardflow/src/lib/api/schema-types.ts` — `ArtifactSummary`, `BoardRunDetail`, `CheckInfo` 型エイリアス追加
- `boardflow/src/components/run-detail/run-detail-content.tsx` — セクションコンポーネント呼び出しに置換、不要 import 削除

### テスト結果
- `pnpm lint` — PASS (Biome format 自動修正後)
- `pnpm typecheck` — PASS
- `pnpm build` — PASS (全ルート正常ビルド)

### ドキュメント更新
- 本 worklog への追記のみ (API/仕様ドキュメント更新不要)

### 実装後の残リスク
- なし。純粋なリファクタリングで挙動変更なし。
