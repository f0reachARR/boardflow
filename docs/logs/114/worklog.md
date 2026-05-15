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

---

## 実装フェーズ (impl agent)

**日時**: 2026-05-15

### 実装内容

計画に沿って7ファイルを変更。計画からの差分として `guards.ts` は完全削除ではなく `isRecord` を残存させた（3コンポーネントが使用中のため）。

#### 変更ファイル一覧

| # | ファイル | 操作 | 内容 |
|---|---|---|---|
| 1 | `boardflow/src/lib/domain/diff-summary-schema.ts` | **新規** | `FileChangesSchema`, `BomChangesSchema`, `CheckEntrySchema`, `ArtifactChangesSchema` の zod schema |
| 2 | `boardflow/src/lib/domain/diff-summary.ts` | **編集** | `guards.ts` の import → schema import に切替。`parseDiffSummary` 内部を `safeParse` ベースに書き換え。interface・シグネチャは維持 |
| 3 | `boardflow/src/lib/domain/guards.ts` | **編集** | `isFileChanges`, `isBomChanges`, `isCheckEntry`, `isArtifactChanges` を削除。`isRecord` のみ残存（`file-changes-section.tsx`, `artifact-changes-section.tsx`, `preview-links-section.tsx` が使用） |
| 4 | `boardflow/src/lib/api/error.ts` | **新規** | `parseApiErrorMessage(err: unknown): string \| null` helper。typeof チェックで `{ error: { message: string } }` を検査 |
| 5 | `boardflow/src/components/run-detail/run-detail-content.tsx` | **編集** | `as Record<...>` キャスト → `parseApiErrorMessage(diffError)` に置換 |
| 6 | `boardflow/src/components/tokens/create-token-dialog.tsx` | **編集** | `as { error?: ... }` キャスト → `parseApiErrorMessage(err)` に置換 |
| 7 | `boardflow/src/lib/api/schema-types.ts` | **編集** | 未使用 `DiffSummary` interface とコメントを削除 |

#### 計画からの差分

- **`guards.ts` 完全削除 → `isRecord` のみ残存**: 計画では「`diff-summary.ts` 以外から import なし」としていたが、実際は `file-changes-section.tsx`, `artifact-changes-section.tsx`, `preview-links-section.tsx` の3ファイルが `isRecord` を import していた。`isRecord` は汎用ユーティリティなので残存が妥当。
- **`parseApiErrorMessage` の実装**: 計画では zod `safeParse` を使う案もあったが、2層の typeof チェックのみのシンプルな実装を採用。zod を import する追加コストに見合わないため。

### テスト結果

| チェック | 結果 |
|---|---|
| `pnpm lint` | ✅ パス (Checked 89 files, No errors) |
| `pnpm typecheck` | ✅ パス (`tsc --noEmit` 成功) |
| `pnpm build` | ✅ パス (Next.js 16.2.4 production build 成功) |

### 残リスク

1. **schema と interface の二重管理**: `diff-summary-schema.ts` の zod schema と `diff-summary.ts` の TypeScript interface が並存。将来 `z.infer<typeof Schema>` に統一すれば解消可能。
2. **API error 構造の backend 依存**: `{ error: { message: string } }` は OpenAPI schema で定義されていない。backend 変更時は `parseApiErrorMessage` が `null` を返し fallback メッセージが表示される（安全側に倒れる）。
3. **`isRecord` の将来移動**: `guards.ts` に `isRecord` のみが残っている状態。必要に応じて `utils.ts` 等に移動する検討が可能。

---

## レビューフェーズ (review agent)

**日時**: 2026-05-15

### レビュー結果

- **判定**: `pr_ready: false`
- **総評**: `diff.summary` の file/bom/artifact 向け runtime parse を zod `safeParse` に寄せ、API error の危険な `as` キャストも helper 化されており、変更範囲は概ね Issue の意図に沿っている。一方で、`checks` フィールドの不正形式が依然として「Data format not recognized」相当の表示に落ちず、空配列として黙って非表示になる経路が残っているため、完了条件の「runtime parse 失敗時の表示が明確に扱われている」は未充足と判断した。

### 調査結果

- ベース差分を確認: `git diff <merge-base>..HEAD` でコード変更 7 ファイルに加え、research 文書 `docs/external/zod-v4-safeparse-unknown-json.md` と `docs/logs/114/worklog.md` の更新を確認。
- `docs/spec.md`、`docs/frontend/summary.md`、`README.md` を確認。今回の変更は frontend 内部リファクタリングの範囲で、追加の仕様/README 更新は不要。
- repository 内に `CONTRIBUTING.md` は存在せず、確認対象なし。
- Web 調査では、zod の `safeParse` は部分バリデーションに有効であり、動的キーは `z.record()` などで扱えることを再確認。今回の方針自体は妥当。

### 重大度順の指摘

1. **[Major] `checks` の parse failure が UI 上で明確に扱われず、Issue の完了条件を満たしていない**
  - `parseDiffSummary()` は `checks` が object である限り、各 entry を `safeParse` 成功分だけ残して `checks` に代入するため、全 entry が不正でも `[]` を返す。`null` にはならない。該当: `boardflow/src/lib/domain/diff-summary.ts`.
  - `ChecksSection` は `summary.checks === null` の場合だけ「Data format not recognized」を表示し、空配列は `return null` で何も出さない。該当: `boardflow/src/components/diff/checks-section.tsx`.
  - `RunDiffSummaryCard` 側も `summary.checks != null && summary.checks.length > 0` のときしか checks を描画せず、不正データ時は無言で消える。該当: `boardflow/src/components/run-detail/run-diff-summary-card.tsx`.
  - 例として backend が `checks: { erc: 123 }` を返した場合、file/bom/artifact と違って「format not recognized」系の fallback が出ず、利用者には「checks が無い」のか「壊れている」のか判別できない。

### 必須修正

- `checks` の不正形式を `null` 扱いにして fallback 表示へ落とすか、少なくとも「entry が存在したが 1 件も parse できなかった」ケースを明示表示に変えること。

### 任意改善

- `parseApiErrorMessage()` は helper 化としては十分だが、計画/research では zod ベース案を採っていたため、worklog 上で「zod を使わない判断理由」はもう少し明確に残しておくと差分説明がしやすい。
- `checks` の parse は `filter(...safeParse(...).success)` ではなく、`safeParse` 結果から `result.data` を明示的に取り出す実装の方が、将来 schema に transform/default が入っても挙動がぶれにくい。

### テスト結果

- `pnpm lint` ✅
- `pnpm typecheck` ✅
- `pnpm build` ✅
- ただし、runtime parse failure を直接検証する unit/component test は追加されていない。

### テスト不足

- `parseDiffSummary()` に対して、`checks` が object だが entry が壊れているケースを検証する unit test がない。
- `parseApiErrorMessage()` に対して、`{ error: { message } }` 以外の shape を fallback へ落とすケースの回帰検知がない。

### ドキュメント確認

- `docs/spec.md`: 影響なし。
- `docs/frontend/summary.md`: 「フォーム/バリデーションに zod を使う」方針と矛盾なし。
- `README.md`: 今回の内部リファクタリングで更新不要。
- `docs/external/zod-v4-safeparse-unknown-json.md`: 実装方針と概ね整合。ただし `checks` の malformed データを明示表示へ落とす点は実装に未反映。

### plan / research / docs との不整合

- research と plan は「不正な形式の場合は既存の `Data format not recognized` 相当の表示に落とす」としているが、`checks` については未達。
- plan 上の `parseApiErrorMessage` は zod ベース案だったが、実装は軽量な手書き helper。これは仕様違反ではないが、計画との差分として明示されているため blocker ではない。

### PR/完了結果

- `pr_ready: false`
- 理由: `checks` malformed 時の fallback 表示が未実装で、Issue #114 の完了条件を満たしていないため。

### 残リスク

- malformed `checks` を silent drop する現状だと、backend schema 逸脱の検知が UI 上で遅れる。
- `parseApiErrorMessage` の期待 shape は OpenAPI に載っていないため、backend 側変更時の退行は unit test なしでは見落としやすい。

### 更新した作業ログパス

- `docs/logs/114/worklog.md`

---

## レビュー修正フェーズ (impl agent - review fix)

**日時**: 2026-05-15

### 修正内容

レビューの必須指摘2件に対応。

#### 1. [Major] checks の malformed データが silent drop される問題

**修正箇所**: `boardflow/src/lib/domain/diff-summary.ts`

**修正前**: `Object.entries().filter(safeParse().success)` で不正 entry を除外。全 entry 失敗でも空配列 `[]` を返していた。

**修正後**: 
- `for...of` で各 entry を `safeParse` し、`result.data` を `parsed` 配列に push
- `rawEntries.length > 0 && parsed.length === 0` の場合に `null` を返す
- これにより `ChecksSection` の `null` チェックが発火し「Data format not recognized」が表示される

#### 2. [任意改善] safeParse の result.data を明示的に使う

**修正箇所**: 同上

`CheckEntrySchema.safeParse(value)` の `result.data` を明示的に `parsed` に push する形に変更。将来 schema に transform/default が入っても安全。

### 追加テスト

**新規ファイル**: `boardflow/src/lib/domain/__tests__/diff-summary.test.ts` (vitest)

テスト観点 (18テスト):
- **checks 正常系**: 全 entry 有効時に全件パースされること
- **checks 部分不正**: 一部不正 entry がある場合、有効分のみ返ること
- **checks 全件不正 (silent drop fix)**: 全 entry 不正時に `null` が返ること（今回の主要修正）
- **checks キー欠落**: `checks` がない場合 `null`
- **checks 非オブジェクト**: string, array, null の場合 `null`
- **checks 空オブジェクト**: `{}` の場合は空配列 `[]`（正常な空）
- **result.data 使用の検証**: extra field が strip されること
- **fileChanges 正常/不正**: safeParse 成功/失敗
- **bomChanges 正常/不正**: safeParse 成功/失敗
- **artifactChanges 正常/不正**: safeParse 成功/失敗
- **edge cases**: non-object, null, array の raw input

### テスト結果

| チェック | 結果 |
|---|---|
| `vitest run` (18 tests) | ✅ 全パス |
| `pnpm lint` | ✅ パス |
| `pnpm typecheck` | ✅ パス |
| `pnpm build` | ✅ パス |

### 残リスク

1. **schema と interface の二重管理**: 変更なし（将来 `z.infer` で統一可能）
2. **API error 構造の backend 依存**: 変更なし（fallback が安全側に倒れる）
3. **`parseApiErrorMessage` のテスト不足**: レビューで任意指摘。今回のスコープ外だが将来追加推奨

### 更新した作業ログパス

- `docs/logs/114/worklog.md`
