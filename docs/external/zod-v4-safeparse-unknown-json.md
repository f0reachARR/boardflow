# zod v4 safeParse による unknown JSON バリデーション

## 要約

zod v4 (`^4.4.3`) の `safeParse` を使って、OpenAPI schema 上の `unknown` フィールド（`diff.summary`）や API エラーレスポンスをランタイムバリデーションする方法の調査。

## 確認した情報

### zod v4 の safeParse API

v3 と同一インターフェース:

```ts
import { z } from 'zod';

const schema = z.object({ name: z.string() });
const result = schema.safeParse(data);
if (result.success) {
  // result.data は型付き
} else {
  // result.error に ZodError
}
```

### フィールド単位の部分バリデーション

全体を一つの schema で `safeParse` すると all-or-nothing になる。フィールド単位で個別に `safeParse` すれば、一部が不正でも他は正常に取得できる:

```ts
const fileChanges = FileChangesSchema.safeParse(raw?.file_changes);
const bomChanges = BomChangesSchema.safeParse(raw?.bom_changes);
return {
  fileChanges: fileChanges.success ? fileChanges.data : null,
  bomChanges: bomChanges.success ? bomChanges.data : null,
};
```

### z.record() での動的キー

`checks` フィールド（`Record<string, CheckEntry>`）は `z.record()` で表現可能:

```ts
const CheckEntrySchema = z.object({
  status_change: z.string(),
  error_delta: z.number(),
  warning_delta: z.number(),
});
const ChecksSchema = z.record(z.string(), CheckEntrySchema);
```

### v4 の主な変更点（safeParse に影響するもの）

- エラーカスタマイズは `error` パラメータに統一
- `ZodError` のissue型名が `z.core.$ZodIssue*` にリネーム
- `safeParse` 自体のシグネチャは変更なし

## BoardFlow への示唆

- 既存の `parseDiffSummary(raw: unknown)` + `guards.ts` の手動 type guard を zod schema に置き換え可能
- `parseDiffSummary` の返り値型 `ParsedDiffSummary` は変更不要 — 消費側のコンポーネントに影響なし
- API error の `as` キャストも zod schema helper で安全化可能

## 採用/不採用判断

**採用**: zod v4 は既存依存 (`^4.4.3`) であり、`safeParse` API は安定。手動 type guard からの移行は自然。

## 制約と pitfall

- フィールド単位の部分バリデーションを行う場合、各フィールドの schema を個別に `safeParse` する必要がある（全体 safeParse だと1フィールドの不正で全体が失敗）
- `z.record(z.string(), CheckEntrySchema)` のパース結果は `Record<string, CheckEntry>` だが、`Object.entries()` で `[string, CheckEntry][]` への変換は引き続き必要
- bundle size: zod v4 は v3 より軽量化されているが、schema 定義のコード量自体は type guard と同程度

## 未解決の疑問

- なし（技術的にはすべて解決可能）

## 参照URL

- https://zod.dev/v4/changelog — zod v4 migration guide
- https://zod.dev — zod 公式ドキュメント
