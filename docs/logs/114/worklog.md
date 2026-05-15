# Issue #114: frontend: unknown JSON への型キャストを zod parse に置き換える

## Issueまでの経緯

OpenAPI schema 上で `summary?: unknown` として定義される `diff.summary` について、フロントエンドでは手動の type guard (`isRecord`, `isFileChanges` 等) と `parseDiffSummary()` 関数を使ってランタイムバリデーションを行っている。しかし API エラーの取り出しでは `as Record<string, unknown>` などの型キャストが残っている。Issue #114 は、これらを zod の `safeParse` に統一して型安全性と変更耐性を向上させるリファクタリング。

## ユーザー要望

- 挙動変更は避け、純粋なリファクタリングに留める
- `diff.summary as DiffSummary` のような `as` キャストを減らす
- runtime parse 失敗時の表示を明確にする
- 既存の正常系表示を維持する

## 調査結果

### 1. 対象ファイルの現状

#### `boardflow/src/components/diff/diff-content.tsx`
- L6: `import { parseDiffSummary } from '@/lib/domain/diff-summary'`
- L75: `const summary = parseDiffSummary(diff.summary)`
- **`as` キャストなし** — 既に `parseDiffSummary(raw: unknown)` 経由で安全にパース済み

#### `boardflow/src/components/run-detail/run-detail-content.tsx`
- **L63-65: API error の `as` キャスト** (主要な対象):
  ```ts
  diffError && (diffError as Record<string, unknown>)?.error
    ? ((diffError as Record<string, { message?: string }>).error?.message ??
        'Failed to load diff data.')
    : null;
  ```
- diff.summary 自体は `RunDiffSummaryCard` にそのまま渡され、そちらで `parseDiffSummary()` を呼んでいる

#### `boardflow/src/components/run-detail/run-diff-summary-card.tsx`
- L50: `const summary = parseDiffSummary(diff.summary)`
- **`as` キャストなし** — 同様に safe

#### `boardflow/src/lib/api/schema-types.ts`
- L29-34: `DiffSummary` interface が手動定義されているが、**import されている箇所は0件** (未使用)
  ```ts
  export interface DiffSummary {
    file_changes?: unknown;
    bom_changes?: unknown;
    checks?: unknown;
    artifacts?: unknown;
  }
  ```

#### `boardflow/src/lib/domain/diff-summary.ts`
- `parseDiffSummary(raw: unknown): ParsedDiffSummary` — 手書きの type guard を使ったパーサー
- `guards.ts` の `isRecord`, `isFileChanges`, `isBomChanges`, `isCheckEntry`, `isArtifactChanges` を使用

#### `boardflow/src/lib/domain/guards.ts`
- 手動の type guard 5関数。各フィールドの `typeof` チェックで構成

### 2. `as` 型キャスト全リスト

| ファイル | 行 | コード断片 | 種別 |
|---|---|---|---|
| `run-detail-content.tsx` | L63 | `diffError as Record<string, unknown>` | API error |
| `run-detail-content.tsx` | L64 | `diffError as Record<string, { message?: string }>` | API error |
| `create-token-dialog.tsx` | L64 | `err as { error?: { message?: string } }` | API error |
| `create-token-dialog.tsx` | L144 | `(e as { message?: string })?.message` | validation error |
| `checks/[checkKind]/page.tsx` | L55-56 | `checkKind as 'erc' \| 'drc'`, `severity as 'error' \| 'warning' \| 'notice' \| undefined` | URL param narrowing (not unknown JSON) |

**注意**: `as DiffSummary` のキャストは **現在ゼロ件**。Issue本文で言及されている箇所は既に `parseDiffSummary(raw: unknown)` に移行済み。

### 3. OpenAPI schema での `summary` 型

`schema.d.ts` L484: `summary?: unknown` — backendが任意のJSONを返すフィールド。

### 4. zod 既存利用パターン

- **バージョン**: `"zod": "^4.4.3"` (zod v4)
- **インポート**: `import { z } from 'zod'` (v4のpackage root)
- **利用箇所**: `create-token-dialog.tsx` のみ
  ```ts
  const createTokenSchema = z.object({
    name: z.string().min(1, '...').max(100, '...'),
  });
  ```
  `.parse()` で使用（form validation）

### 5. 「Data format not recognized」表示

parse 失敗時の fallback 表示。以下のファイルで null チェックにより表示:
- `checks-section.tsx` L19
- `run-diff-summary-card.tsx` L74, L86, L111
- `artifact-changes-section.tsx` L21
- `bom-changes-section.tsx` L20
- `file-changes-section.tsx` L21

`parseDiffSummary()` が各フィールドに `null` を返すと、消費側コンポーネントが「data format not recognized」テキストを表示する。この仕組みは zod 化後も同じロジックで維持可能。

### 6. DiffSummary 型の構造

```ts
// 入力 (unknown JSON from backend)
{
  file_changes?: { added: number; removed: number; changed: number; unchanged: number }
  bom_changes?: { added: number; removed: number; changed: number }
  checks?: Record<string, { status_change: string; error_delta: number; warning_delta: number }>
  artifacts?: { added: number; removed: number; changed: number }
}

// パース後 (ParsedDiffSummary)
{
  fileChanges: FileChanges | null
  bomChanges: BomChanges | null
  checks: [string, CheckEntry][] | null
  artifactChanges: ArtifactChanges | null
}
```

### 7. zod v4 での safeParse API

zod v4 では `safeParse` API は v3 と同一:
```ts
const result = schema.safeParse(data);
if (result.success) { result.data } else { result.error }
```

zod v4 の主な変更はエラーカスタマイズの `error` パラメータ統一、パフォーマンス改善、JSON Schema 変換。`safeParse` のインターフェースは互換。

## 計画 (実装エージェント向け)

### 対応範囲

1. **`diff-summary.ts` + `guards.ts` → zod schema 化**
   - `FileChangesSchema`, `BomChangesSchema`, `CheckEntrySchema`, `ArtifactChangesSchema` を zod で定義
   - `DiffSummarySchema` を `z.object({ ... }).partial()` で定義
   - `parseDiffSummary()` 内で `safeParse` を使用し、失敗時は各フィールド null を返す（既存挙動維持）
   - `guards.ts` は zod schema に吸収されるため削除候補

2. **API error helper 化**
   - `run-detail-content.tsx` L63-65 の `as` キャスト → zod schema or helper 関数
   - `create-token-dialog.tsx` L64 の `as` キャスト → 同じ helper を共有
   - 例: `parseApiErrorMessage(err: unknown): string | null`

3. **未使用 `DiffSummary` interface 削除**
   - `schema-types.ts` の `DiffSummary` interface は import 0件。zod schema に置き換えるなら削除。

4. **対応しない箇所**
   - `checks/[checkKind]/page.tsx` の `as 'erc' | 'drc'` — URL param narrowing で unknown JSON とは無関係
   - `create-token-dialog.tsx` L144 の `(e as { message?: string })` — zod validation error の表示で文脈が異なる

### リスク
- zod v4 の `z.record()` で checks フィールドの `Record<string, CheckEntry>` を表現可能だが、`Object.entries()` 変換が必要な点は変わらない
- `parseDiffSummary` のシグネチャ (`raw: unknown → ParsedDiffSummary`) が変わらなければ、消費側コンポーネントの変更はゼロ

## 結論ステータス

**`implementation_required`**

理由:
- `as` キャストが残っている箇所(主に API error 関連)の修正が必要
- `guards.ts` の手動 type guard を zod schema に置き換えるコード変更が必要
- ただし `diff.summary` 自体は既に `parseDiffSummary(raw: unknown)` で安全にパースされており、Issueで想定されていたより影響範囲は小さい

## 参照URL

- https://zod.dev/v4/changelog — zod v4 migration guide
- https://zod.dev — zod 公式ドキュメント

## 残リスク

- `parseDiffSummary` のフィールド単位 safeParse では、一つのフィールドが不正でも他は正常に表示される現在の挙動を維持する必要がある（全体 safeParse だとall-or-nothing になる可能性）
- API error の構造は OpenAPI schema で定義されておらず、backend 実装依存。zod schema を書いても backend 側の変更で壊れうる点は変わらない

---

## 計画フェーズ (plan agent)

**日時**: 2026-05-15

### 目的

- `guards.ts` の手動 type guard を zod schema に置き換え、バリデーションロジックを宣言的にする
- API error の `as` キャスト（2箇所）を zod-based helper に置き換え、型安全性を向上させる
- 未使用の `DiffSummary` interface を削除する

### 非目的

- 挙動の変更（純粋リファクタリング）
- `checks/[checkKind]/page.tsx` の URL param narrowing `as`（unknown JSON とは無関係）
- `create-token-dialog.tsx` L144 の validation error 表示の `as`（zod validation error の型で文脈が異なる）
- diff 表示コンポーネント側の変更（`parseDiffSummary` のシグネチャを維持するため不要）

### 受け入れ条件

1. `diff.summary` の手動 type guard (`guards.ts`) が zod schema による `safeParse` に置換されている
2. `run-detail-content.tsx` の `diffError as Record<...>` が helper 関数に置換されている
3. `create-token-dialog.tsx` の `err as { error?: ... }` が同じ helper 関数に置換されている
4. `schema-types.ts` の未使用 `DiffSummary` interface が削除されている
5. parse 失敗時の fallback 表示（null → 「Data format not recognized」）が維持されている
6. `pnpm typecheck` / `pnpm lint` / `pnpm build` がすべてパスする

### 詳細要件

#### ファイル変更一覧

| # | ファイル | 操作 | 内容 |
|---|---|---|---|
| 1 | `boardflow/src/lib/domain/diff-summary-schema.ts` | **新規作成** | zod schema 定義 (`FileChangesSchema`, `BomChangesSchema`, `CheckEntrySchema`, `ArtifactChangesSchema`) |
| 2 | `boardflow/src/lib/domain/diff-summary.ts` | **編集** | `guards.ts` の import → schema import に変更。`parseDiffSummary` 内部を `safeParse` ベースに書き換え。interface と `ParsedDiffSummary` はそのまま維持。 |
| 3 | `boardflow/src/lib/domain/guards.ts` | **削除** | zod schema に吸収。`diff-summary.ts` 以外から import されていないことを確認済み。 |
| 4 | `boardflow/src/lib/api/error.ts` | **新規作成** | `parseApiErrorMessage(err: unknown): string \| null` helper。zod の `safeParse` で `{ error: { message: string } }` を検査。 |
| 5 | `boardflow/src/components/run-detail/run-detail-content.tsx` | **編集** | L62-66 の `as` キャストを `parseApiErrorMessage(diffError)` に置換。fallback メッセージ `'Failed to load diff data.'` を維持。 |
| 6 | `boardflow/src/components/tokens/create-token-dialog.tsx` | **編集** | L64 の `as` キャストを `parseApiErrorMessage(err)` に置換。fallback メッセージ `'トークンの作成に失敗しました'` を維持。 |
| 7 | `boardflow/src/lib/api/schema-types.ts` | **編集** | L29-35 の未使用 `DiffSummary` interface とそのコメントを削除。 |

#### 変更順序（依存関係）

```
1. diff-summary-schema.ts (新規) — 依存なし
2. diff-summary.ts (編集) — 1 に依存
3. guards.ts (削除) — 2 の完了後
4. error.ts (新規) — 依存なし (1 と並行可能)
5. run-detail-content.tsx (編集) — 4 に依存
6. create-token-dialog.tsx (編集) — 4 に依存
7. schema-types.ts (編集) — 依存なし (任意のタイミング)
```

### 設計方針

#### 1. `diff-summary-schema.ts` の設計

```ts
import { z } from 'zod';

export const FileChangesSchema = z.object({
  added: z.number(),
  removed: z.number(),
  changed: z.number(),
  unchanged: z.number(),
});

export const BomChangesSchema = z.object({
  added: z.number(),
  removed: z.number(),
  changed: z.number(),
});

export const CheckEntrySchema = z.object({
  status_change: z.string(),
  error_delta: z.number(),
  warning_delta: z.number(),
});

export const ArtifactChangesSchema = z.object({
  added: z.number(),
  removed: z.number(),
  changed: z.number(),
});
```

- interface (`FileChanges`, `BomChanges`, etc.) は `diff-summary.ts` にそのまま残す（`z.infer` で置換も可能だが、既存の export を壊さないため維持）
- schema と interface の二重管理が気になる場合は、将来的に `z.infer<typeof Schema>` に寄せる拡張が可能

#### 2. `parseDiffSummary` の書き換え

```ts
import { FileChangesSchema, BomChangesSchema, CheckEntrySchema, ArtifactChangesSchema } from './diff-summary-schema';

export function parseDiffSummary(raw: unknown): ParsedDiffSummary {
  const obj = typeof raw === 'object' && raw !== null && !Array.isArray(raw)
    ? (raw as Record<string, unknown>)
    : {};

  const fileChanges = FileChangesSchema.safeParse(obj.file_changes);
  const bomChanges = BomChangesSchema.safeParse(obj.bom_changes);
  const artifactChanges = ArtifactChangesSchema.safeParse(obj.artifacts);

  const checksObj = typeof obj.checks === 'object' && obj.checks !== null && !Array.isArray(obj.checks)
    ? obj.checks as Record<string, unknown>
    : null;

  const checks = checksObj
    ? Object.entries(checksObj)
        .map(([key, val]) => {
          const result = CheckEntrySchema.safeParse(val);
          return result.success ? ([key, result.data] as [string, CheckEntry]) : null;
        })
        .filter((entry): entry is [string, CheckEntry] => entry !== null)
    : null;

  return {
    fileChanges: fileChanges.success ? fileChanges.data : null,
    bomChanges: bomChanges.success ? bomChanges.data : null,
    checks,
    artifactChanges: artifactChanges.success ? artifactChanges.data : null,
  };
}
```

- **シグネチャ不変**: `(raw: unknown) → ParsedDiffSummary`
- **フィールド単位 safeParse**: 1フィールドが不正でも他は正常に返る（既存挙動維持）
- `isRecord` の inline 化: `guards.ts` 削除後も record チェックが必要な箇所は最小限のインラインチェックで対応

#### 3. `parseApiErrorMessage` helper の設計

```ts
import { z } from 'zod';

const ApiErrorSchema = z.object({
  error: z.object({
    message: z.string(),
  }),
});

export function parseApiErrorMessage(err: unknown): string | null {
  const result = ApiErrorSchema.safeParse(err);
  return result.success ? result.data.error.message : null;
}
```

- 呼び出し側: `parseApiErrorMessage(diffError) ?? 'Failed to load diff data.'`
- `create-token-dialog.tsx`: `parseApiErrorMessage(err) ?? 'トークンの作成に失敗しました'`

### 影響範囲

- **変更が必要なコンポーネント**: `run-detail-content.tsx`, `create-token-dialog.tsx` — いずれも import と1行の置換のみ
- **変更不要なコンポーネント**: `diff-content.tsx`, `run-diff-summary-card.tsx`, `checks-section.tsx`, `file-changes-section.tsx`, `bom-changes-section.tsx`, `artifact-changes-section.tsx` — `parseDiffSummary` のシグネチャが不変のため
- **削除**: `guards.ts` (5関数)、`DiffSummary` interface (schema-types.ts)
- **新規**: `diff-summary-schema.ts`, `error.ts`

### テスト観点

1. **既存の正常系表示の維持**: diff summary の各セクション（file changes, bom changes, checks, artifacts）が正常に表示されること
2. **parse 失敗時の fallback**: 不正なデータ形式の場合に「Data format not recognized」相当の表示が出ること（各フィールドが null になること）
3. **API error 表示**: diff ロード失敗時にエラーメッセージが正しく表示されること
4. **トークン作成エラー**: サーバーエラー時のメッセージが正しく表示されること
5. **型チェック**: `pnpm typecheck` がパスすること
6. **lint**: `pnpm lint` がパスすること
7. **ビルド**: `pnpm build` がパスすること

### ドキュメント更新対象

- `docs/logs/114/worklog.md` — 実装フェーズの結果を追記
- その他ドキュメント更新は不要（内部リファクタリングのため）

### 実装要否

**`implementation_required`**

### 未解決の疑問

なし。researchフェーズで技術的疑問はすべて解消済み。ユーザー要望（挙動変更なし）も明確。

### 残リスク

1. **schema と interface の二重管理**: `diff-summary-schema.ts` に zod schema、`diff-summary.ts` に TypeScript interface が並存する。将来 `z.infer` に統一する拡張は容易だが、今回は既存 export を壊さないためそのまま。
2. **API error 構造の backend 依存**: `{ error: { message: string } }` は OpenAPI schema に定義されていない。backend 変更時は `parseApiErrorMessage` が `null` を返し fallback メッセージが表示される（安全側に倒れる）。

### 更新した作業ログパス

`docs/logs/114/worklog.md`
