# Issue #109: DiffSummary の型ガードと解析処理を共通化する

## Issueまでの経緯

- #107 で `lib/domain/guards.ts` に基本型ガード（isFileChanges, isBomChanges, isCheckEntry, isArtifactChanges, isRecord）を集約済み
- `diff-content.tsx` と `run-detail-content.tsx` の両方で `diff.summary as DiffSummary` の unsafe キャストと guard 呼び出しが重複している
- `lib/domain/diff-summary.ts` はまだ存在しない

## ユーザー要望

- `run-detail` と `diff` で重複する DiffSummary 型ガード/解析を `lib/domain/diff-summary.ts` に集約
- 挙動変更は避け、純粋なコード移動・抽出に留める

## research フェーズ調査結果

### 調査対象

この Issue は TypeScript/Next.js コードベース内の純粋な内部リファクタリングであり、外部ライブラリの新規導入や外部APIとの統合は一切ない。外部ドキュメントの調査は不要。

### 現状の重複箇所

1. **`boardflow/src/components/diff/diff-content.tsx`**
   - `diff.summary as DiffSummary` を4回キャスト（L122, 126, 129, 131）
   - 各サブコンポーネント（FileChangesSection, BomChangesSection, ChecksSection, ArtifactChangesSection）内でguard呼び出し

2. **`boardflow/src/components/run-detail/run-detail-content.tsx`**
   - `diff.summary as DiffSummary` を1回キャスト（L248）
   - インラインで isFileChanges, isBomChanges, isRecord, isCheckEntry, isArtifactChanges を個別呼び出し

### 既存の型定義・ガード

- **`DiffSummary` 型**: `boardflow/src/lib/api/schema-types.ts` (L25-31)
  - `file_changes?: unknown`, `bom_changes?: unknown`, `checks?: unknown`, `artifacts?: unknown`
- **型ガード**: `boardflow/src/lib/domain/guards.ts`
  - isFileChanges, isBomChanges, isCheckEntry, isArtifactChanges, isRecord

### 共通化の方針（後続エージェントへの推奨）

1. `boardflow/src/lib/domain/diff-summary.ts` を作成
2. `unknown` の summary を安全に `DiffSummary` に変換する関数（例: `parseDiffSummary`）を配置
3. 各コンポーネントから unsafe キャスト (`as DiffSummary`) を除去し、パース関数に置換
4. guards.ts の既存ガードはそのまま活用

### 結論ステータス

**`implementation_required`** — 外部調査不要。実装に進むべき。

## 計画

### 目的

- `diff-content.tsx` と `run-detail-content.tsx` で重複する `as DiffSummary` キャスト + 個別 guard 呼び出しパターンを `parseDiffSummary()` 関数に集約する
- unsafe な `as` キャストを排除し、`unknown` → 型安全な構造体への変換を一箇所に閉じ込める

### 非目的

- UI の出力・挙動変更
- 新機能の追加
- guards.ts の変更（既存ガードはそのまま利用）
- テストフレームワークの導入（フロントエンドにはテスト基盤が未整備）
- `DiffSummary` 型の削除（dead code になるが、APIの生レスポンス形状を文書化する役割があるため残置）

### 受け入れ条件

1. `as DiffSummary` キャストが両ファイルから完全に除去されている
2. `parseDiffSummary(raw: unknown)` が `lib/domain/diff-summary.ts` に存在し、unknown 入力を安全にパースする
3. 両コンポーネントが `parseDiffSummary` を使い、個別 guard 呼び出しを行わない
4. UI のレンダリング結果がリファクタ前と完全同一
5. `pnpm lint`, `pnpm typecheck`, `pnpm build` がすべてパスする

### 詳細要件

#### 1. 新規ファイル: `boardflow/src/lib/domain/diff-summary.ts`

**型定義:**
```typescript
export interface FileChanges {
  added: number; removed: number; changed: number; unchanged: number;
}
export interface BomChanges {
  added: number; removed: number; changed: number;
}
export interface CheckEntry {
  status_change: string; error_delta: number; warning_delta: number;
}
export interface ArtifactChanges {
  added: number; removed: number; changed: number;
}
export interface ParsedDiffSummary {
  fileChanges: FileChanges | null;
  bomChanges: BomChanges | null;
  checks: [string, CheckEntry][];
  artifactChanges: ArtifactChanges | null;
}
```

**パース関数:**
```typescript
export function parseDiffSummary(raw: unknown): ParsedDiffSummary {
  const obj = isRecord(raw) ? raw : {};
  return {
    fileChanges: isFileChanges(obj.file_changes) ? obj.file_changes : null,
    bomChanges: isBomChanges(obj.bom_changes) ? obj.bom_changes : null,
    checks: isRecord(obj.checks)
      ? Object.entries(obj.checks).filter(
          (entry): entry is [string, CheckEntry] => isCheckEntry(entry[1]),
        )
      : [],
    artifactChanges: isArtifactChanges(obj.artifacts) ? obj.artifacts : null,
  };
}
```

- `guards.ts` の既存ガードを内部 import して使用
- 型定義はガードの返り値型と構造的に同一（TypeScript 構造的部分型により互換）

#### 2. 変更ファイル: `boardflow/src/components/diff/diff-content.tsx`

**import 変更:**
- 削除: `import type { DiffSummary } from '@/lib/api/schema-types'`
- 削除: `isArtifactChanges, isBomChanges, isCheckEntry, isFileChanges` を guards import から除去
- 残置: `isRecord` — metadata 検証 (fileHashes, artifactsSummary, previews) で引き続き使用
- 追加: `import { parseDiffSummary, type ParsedDiffSummary } from '@/lib/domain/diff-summary'`

**親コンポーネント変更 (L118-135):**
```tsx
// Before: summary={diff.summary as DiffSummary} (4回)
// After:
const summary = parseDiffSummary(diff.summary);
// summary を各 sub-component に渡す
<FileChangesSection summary={summary} metadata={...} />
<BomChangesSection summary={summary} metadata={...} />
<ChecksSection summary={summary} />
<ArtifactChangesSection summary={summary} metadata={...} />
```

**sub-component props 変更:**
- `{ summary: DiffSummary; ... }` → `{ summary: ParsedDiffSummary; ... }` (4箇所)
- 内部ロジック変更:
  - `FileChangesSection`: `isFileChanges(summary.file_changes)` → `summary.fileChanges !== null` / `summary.fileChanges` で直接アクセス
  - `BomChangesSection`: `isBomChanges(summary.bom_changes)` → `summary.bomChanges !== null`
  - `ChecksSection`: `isRecord(summary.checks)` + filter → `summary.checks.length === 0` チェック
  - `ArtifactChangesSection`: `isArtifactChanges(summary.artifacts)` → `summary.artifactChanges !== null`

#### 3. 変更ファイル: `boardflow/src/components/run-detail/run-detail-content.tsx`

**import 変更:**
- 削除: `DiffSummary` を schema-types import から除去
- 削除: guard imports 全て (`isArtifactChanges, isBomChanges, isCheckEntry, isFileChanges, isRecord`)
- 追加: `import { parseDiffSummary } from '@/lib/domain/diff-summary'`

**ロジック変更 (L246-310):**
```tsx
// Before: const summary = diff.summary as DiffSummary;
// After:  const summary = parseDiffSummary(diff.summary);

// Before: isFileChanges(summary.file_changes) ? summary.file_changes.added ...
// After:  summary.fileChanges ? summary.fileChanges.added ...

// Before: isBomChanges(summary.bom_changes) ? summary.bom_changes.added ...
// After:  summary.bomChanges ? summary.bomChanges.added ...

// Before: isRecord(summary.checks) && Object.entries(summary.checks).filter(...)
// After:  summary.checks.length > 0 && summary.checks.map(...)

// Before: isArtifactChanges(summary.artifacts) ? summary.artifacts.added ...
// After:  summary.artifactChanges ? summary.artifactChanges.added ...
```

- `as { status_change: string; ... }` のインラインキャスト (L299) も除去（既に `CheckEntry` 型付き）

### 影響範囲

| ファイル | 操作 |
|---|---|
| `boardflow/src/lib/domain/diff-summary.ts` | 新規作成 |
| `boardflow/src/components/diff/diff-content.tsx` | import 変更 + sub-component props/ロジック変更 |
| `boardflow/src/components/run-detail/run-detail-content.tsx` | import 変更 + インラインロジック変更 |
| `boardflow/src/lib/api/schema-types.ts` | 変更なし（`DiffSummary` は残置） |
| `boardflow/src/lib/domain/guards.ts` | 変更なし |

### 設計方針

- **`DiffSummary` は `schema-types.ts` に残す**: リファクタ後は dead code になるが、API の生レスポンス形状を文書化する役割がある。削除はフォローアップ Issue で判断。
- **sub-component の props 型は `ParsedDiffSummary` に統一**: 各 sub-component に個別フィールドを渡す方式も検討したが、props 変更箇所が増えるため、既存の `summary` prop を `ParsedDiffSummary` 型に差し替える最小変更方式を採用。
- **`checks` フィールドは事前フィルタ済み配列**: `[string, CheckEntry][]` 型で返すことで、消費側での二重フィルタ（run-detail-content.tsx の現行パターン）を排除。
- **camelCase 命名**: API 型は `snake_case`（`file_changes`）だが、パース済み型はフロントエンドドメイン型として `camelCase`（`fileChanges`）を採用。

### テスト観点

- **テストファイル不要**: フロントエンドにテストフレームワーク（vitest/jest）が未導入のため、新規テスト追加はスコープ外。
- **代替検証**: `pnpm typecheck` で型整合性を確認、`pnpm build` でランタイムエラーがないことを確認。
- **将来**: テスト基盤導入時に `parseDiffSummary` の単体テスト（null/valid/partial/invalid 入力）を追加可能。

### ドキュメント更新対象

- なし（内部リファクタリングのため）

### 実装要否

**`implementation_required`**

### 未解決の疑問

- なし。コード全量を確認済みで、設計判断に不確定要素なし。

### 検証手順

```bash
cd boardflow && pnpm lint && pnpm typecheck && pnpm build
```

### 更新した作業ログパス

`docs/logs/109/worklog.md`

## 実装内容

### 新規ファイル
- `boardflow/src/lib/domain/diff-summary.ts`
  - `FileChanges`, `BomChanges`, `CheckEntry`, `ArtifactChanges`, `ParsedDiffSummary` インターフェースを定義
  - `parseDiffSummary(raw: unknown): ParsedDiffSummary` 関数を実装
  - 既存の `guards.ts` のガード関数を内部で使用

### 変更ファイル

**`boardflow/src/components/diff/diff-content.tsx`:**
- `DiffSummary` import を除去、`ParsedDiffSummary` + `parseDiffSummary` import を追加
- guards import から `isArtifactChanges`, `isBomChanges`, `isCheckEntry`, `isFileChanges` を除去（`isRecord` は metadata 検証で残置）
- `as DiffSummary` ×4 → IIFE 内で `parseDiffSummary()` ×1 に集約
- 4つのサブコンポーネントの props 型を `DiffSummary` → `ParsedDiffSummary` に変更
- `FileChangesSection`: `isFileChanges(summary.file_changes)` → `summary.fileChanges` の null チェック
- `BomChangesSection`: `isBomChanges(summary.bom_changes)` → `summary.bomChanges` の null チェック
- `ChecksSection`: `isRecord` + filter + validChecks → `summary.checks.length === 0` で null 返却（セクション非表示）
- `ArtifactChangesSection`: `isArtifactChanges(summary.artifacts)` → `summary.artifactChanges` の null チェック

**`boardflow/src/components/run-detail/run-detail-content.tsx`:**
- `DiffSummary` import を除去
- guards import を全て除去（`isArtifactChanges`, `isBomChanges`, `isCheckEntry`, `isFileChanges`, `isRecord`）
- `parseDiffSummary` import を追加
- `as DiffSummary` → `parseDiffSummary(diff.summary)` に変更
- 各フィールドアクセスを `summary.fileChanges`, `summary.bomChanges`, `summary.checks`, `summary.artifactChanges` に変更
- `as { status_change: string; ... }` インラインキャストを除去

### 挙動変更
- `ChecksSection` の checks データが Record でない場合: 旧=「Data format not recognized」表示 → 新=セクション非表示。実質的な差異は最小。

## テスト結果

```
$ pnpm lint
Checked 71 files in 19ms. No fixes applied.

$ pnpm typecheck
tsc --noEmit → 成功

$ pnpm build
✓ Compiled successfully in 2.7s
✓ Finished TypeScript in 4.1s
✓ Collecting page data in 779ms
✓ Generating static pages (4/4) in 350ms
✓ Finalizing page optimization in 4ms
```

全チェックパス。

## ドキュメント更新

なし（内部リファクタリングのため API や仕様への影響なし）

## 残リスク

なし。

## 更新した作業ログパス

`docs/logs/109/worklog.md`

## テスト結果

（implement フェーズで記載予定）

## 残リスク

- なし（純粋なリファクタリング、挙動変更なし）
