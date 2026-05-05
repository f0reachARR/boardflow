# TanStack Form v1 + Chakra UI v3 統合調査

## 要約

TanStack Form v1 はヘッドレスフォームライブラリであり、Chakra UI v3 との統合を公式にサポートしている。zod 3.24+ は Standard Schema を実装済みのため、`@tanstack/zod-form-adapter` は不要で zod スキーマを直接 validators に渡せる。TanStack Query の mutation との連携は `onSubmit` 内で `mutateAsync` を呼ぶ公式パターンが確立されている。React 19 互換。

## 確認した情報

### 1. パッケージインストール

**必要なパッケージ:**
```bash
pnpm add @tanstack/react-form zod
```

**不要なパッケージ（廃止・不要）:**
- `@tanstack/zod-form-adapter` — zod 3.24+ が Standard Schema を実装しているため不要（[GitHub Issue #1136](https://github.com/TanStack/form/issues/1136)）
- `@tanstack/react-form-nextjs` — Server Actions / SSR 統合が必要な場合のみ。今回のフォーム（Client Component ダイアログ）には不要

**バージョン情報（2026-05-05 時点）:**
- `@tanstack/react-form`: v1.29.1（最新）
- React 19 互換確認済み（公式 examples が React 19 を使用）

### 2. TanStack Form v1 基本構造

```tsx
import { useForm } from '@tanstack/react-form';

const form = useForm({
  defaultValues: {
    name: '',
  },
  onSubmit: async ({ value }) => {
    // value は型安全（defaultValues から推論される）
    console.log(value);
  },
});

// JSX 内
<form
  onSubmit={(e) => {
    e.preventDefault();
    e.stopPropagation();
    form.handleSubmit();
  }}
>
  <form.Field
    name="name"
    children={(field) => (
      <>
        <input
          value={field.state.value}
          onChange={(e) => field.handleChange(e.target.value)}
          onBlur={field.handleBlur}
        />
        {!field.state.meta.isValid && (
          <em>{field.state.meta.errors.join(', ')}</em>
        )}
      </>
    )}
  />
</form>
```

**主要コンセプト:**
- `useForm` でフォームインスタンスを作成
- `form.Field` で各フィールドを宣言（render props パターン）
- `field.state.value` / `field.handleChange` / `field.handleBlur` でバインド
- `field.state.meta.errors` でエラー表示
- `form.Subscribe` で canSubmit / isSubmitting を監視
- `form.handleSubmit()` でサブミットをトリガー

### 3. zod バリデーション統合（Standard Schema）

zod 3.24+ は Standard Schema をネイティブサポートしているため、アダプター不要で直接渡せる。

**フォームレベルバリデーション:**
```tsx
import { z } from 'zod';
import { useForm } from '@tanstack/react-form';

const createTokenSchema = z.object({
  name: z.string().min(1, '名前は必須です').max(100, '名前は100文字以内で入力してください'),
});

const form = useForm({
  defaultValues: { name: '' },
  validators: {
    onChange: createTokenSchema,
  },
  onSubmit: async ({ value }) => { /* ... */ },
});
```

**フィールドレベルバリデーション:**
```tsx
<form.Field
  name="name"
  validators={{
    onChange: z.string().min(1, '名前は必須です').max(100, '100文字以内'),
  }}
  children={(field) => (
    // ...
  )}
/>
```

**注意点:**
- バリデーション時にはスキーマの transform 結果は反映されない。`onSubmit` で変換が必要な場合は `schema.parse(value)` を明示的に呼ぶ
- フォームレベルの validators でフィールドレベルのエラーを返すことも可能（`fields` キー）
- フィールドレベルの validators はフォームレベルのものを上書きする

### 4. TanStack Query mutation 連携パターン

公式 Query Integration example の推奨パターン:

```tsx
import { useForm } from '@tanstack/react-form';
import { useQueryClient } from '@tanstack/react-query';
import { $api } from '@/lib/api/react-query';

function CreateTokenForm({ repositoryId }: { repositoryId: string }) {
  const queryClient = useQueryClient();

  // openapi-react-query の mutation を使用
  const { mutateAsync, isPending } = $api.useMutation(
    'post',
    '/api/v1/repositories/{github_repository_id}/api-tokens',
  );

  const form = useForm({
    defaultValues: { name: '' },
    validators: {
      onChange: z.object({
        name: z.string().min(1, '名前は必須です').max(100, '100文字以内'),
      }),
    },
    onSubmit: async ({ value }) => {
      // mutation を onSubmit 内で呼び出す
      const data = await mutateAsync({
        params: { path: { github_repository_id: Number(repositoryId) } },
        body: { name: value.name.trim() },
      });
      // 成功後の処理
      queryClient.invalidateQueries({
        queryKey: ['get', '/api/v1/repositories/{github_repository_id}/api-tokens'],
      });
      return data;
    },
  });
  // ...
}
```

**ポイント:**
- `mutateAsync` を使って `onSubmit` 内で await する
- mutation の `onSuccess`/`onError` は mutation 側ではなく form の `onSubmit` 内で処理可能
- `formApi.reset()` でフォームをリセット
- mutation の `isPending` は `form.state.isSubmitting` で代替可能（ただし mutation 固有の isPending も利用可能）

### 5. Chakra UI v3 + TanStack Form 統合パターン

公式ドキュメント（UI Libraries ガイド）にChakra UI v3の統合例がある。

**Input との統合:**
```tsx
<form.Field
  name="name"
  children={({ state, handleChange, handleBlur }) => (
    <Input
      value={state.value}
      onChange={(e) => handleChange(e.target.value)}
      onBlur={handleBlur}
      placeholder="Enter your name"
    />
  )}
/>
```

**Chakra UI v3 Field.Root + エラー表示の統合:**
```tsx
import { Field, Input } from '@chakra-ui/react';

<form.Field
  name="name"
  validators={{
    onChange: z.string().min(1, '名前は必須です').max(100, '100文字以内'),
  }}
  children={(field) => (
    <Field.Root invalid={!field.state.meta.isValid && field.state.meta.isTouched}>
      <Field.Label>トークン名</Field.Label>
      <Input
        placeholder="例: CI用トークン"
        value={field.state.value}
        onChange={(e) => field.handleChange(e.target.value)}
        onBlur={field.handleBlur}
        maxLength={100}
      />
      {field.state.meta.isTouched && !field.state.meta.isValid && (
        <Field.ErrorText>
          {field.state.meta.errors.join(', ')}
        </Field.ErrorText>
      )}
    </Field.Root>
  )}
/>
```

**Checkbox との統合（Chakra UI v3）:**
```tsx
<form.Field
  name="isChecked"
  children={({ state, handleChange, handleBlur }) => (
    <Checkbox.Root
      checked={state.value}
      onCheckedChange={(details) => handleChange(!!details.checked)}
      onBlur={handleBlur}
    >
      <Checkbox.HiddenInput />
      <Checkbox.Control />
      <Checkbox.Label>同意する</Checkbox.Label>
    </Checkbox.Root>
  )}
/>
```

**注意: `Field` 名前の衝突**

TanStack Form の `form.Field` と Chakra UI の `Field` コンポーネントの名前が衝突する。対処法:
1. TanStack Form は `form.Field`（useForm の返り値から取得）として使い、Chakra UI は `Field` として import する → **衝突しない**
2. destructure する場合は rename が必要:
   ```tsx
   const { Field: FormField } = useForm({ ... });
   ```
3. 推奨: `form.Field` をそのまま使う（公式推奨パターン）

### 6. サブミットボタンの状態管理

```tsx
<form.Subscribe
  selector={(state) => [state.canSubmit, state.isSubmitting]}
  children={([canSubmit, isSubmitting]) => (
    <Button
      type="submit"
      colorPalette="blue"
      disabled={!canSubmit}
      loading={isSubmitting}
    >
      作成
    </Button>
  )}
/>
```

### 7. フォームリセット

```tsx
// onSubmit 完了後にリセット
onSubmit: async ({ value, formApi }) => {
  await mutateAsync(/* ... */);
  formApi.reset();
}

// 外部からリセット（ダイアログ close 時）
const handleClose = () => {
  form.reset();
  onOpenChange(false);
};
```

### 8. サーバーエラーのフォームへの反映

mutation のエラー（APIエラー）をフォームに反映するパターン:

```tsx
onSubmit: async ({ value }) => {
  try {
    await mutateAsync({ /* ... */ });
  } catch (err) {
    // フォームレベルのエラーを設定
    // form.state.errorMap には直接設定できないため、
    // useState でサーバーエラーを別管理するか、
    // form の onSubmitAsync validator を使う
    throw err; // TanStack Form が自動でエラーを capture
  }
}
```

より洗練されたパターン（`onSubmitAsync` validator 使用）:
```tsx
const form = useForm({
  defaultValues: { name: '' },
  validators: {
    onChange: createTokenSchema,
    onSubmitAsync: async ({ value }) => {
      try {
        await mutateAsync({
          params: { path: { github_repository_id: Number(repositoryId) } },
          body: { name: value.name.trim() },
        });
        return null; // no errors
      } catch (err) {
        return err.error?.message ?? 'トークンの作成に失敗しました';
      }
    },
  },
  onSubmit: async ({ value }) => {
    // validators.onSubmitAsync が通った後に呼ばれる
    // ここでは成功後の処理のみ
    queryClient.invalidateQueries({ /* ... */ });
  },
});
```

**ただし注意:** `onSubmit` と `onSubmitAsync` validator の使い分けが複雑になるため、BoardFlow のように単純なフォームでは `onSubmit` 内で try/catch し、エラーは `useState` で管理する方がシンプル。

### 9. BoardFlow 固有: CreateTokenDialog の移行パターン

**移行前（現状）:**
- `useState` x 3: name, error, createdToken
- 手動バリデーション（trim + 長さチェック）
- `mutate` を `handleCreate` 内で呼び出し

**移行後（実装済み）:**
```tsx
'use client';

import { useForm } from '@tanstack/react-form';
import { z } from 'zod';
import { Field, Input, Button, Dialog, HStack, /* ... */ } from '@chakra-ui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { $api } from '@/lib/api/react-query';

// trim() は使わない — Standard Schema では transform 扱いのため onSubmit で明示的に trim する
const createTokenSchema = z.object({
  name: z.string()
    .min(1, '名前は1文字以上で入力してください')
    .max(100, '名前は100文字以内で入力してください'),
});

export function CreateTokenDialog({ repositoryId, open, onOpenChange }: Props) {
  const [serverError, setServerError] = useState('');
  const [createdToken, setCreatedToken] = useState<string | null>(null);
  const queryClient = useQueryClient();

  const { mutateAsync } = $api.useMutation(
    'post',
    '/api/v1/repositories/{github_repository_id}/api-tokens',
  );

  const form = useForm({
    defaultValues: { name: '' },
    validators: {
      onChange: createTokenSchema,
    },
    onSubmit: async ({ value }) => {
      setServerError('');
      try {
        const data = await mutateAsync({
          params: { path: { github_repository_id: Number(repositoryId) } },
          body: { name: value.name.trim() }, // 明示的に trim
        });
        if (!data?.token) {
          setServerError('トークンの作成に失敗しました');
          return;
        }
        setCreatedToken(data.token);
        queryClient.invalidateQueries({
          queryKey: ['get', '/api/v1/repositories/{github_repository_id}/api-tokens'],
        });
      } catch (err: unknown) {
        const apiErr = err as { error?: { message?: string } };
        setServerError(apiErr.error?.message ?? 'トークンの作成に失敗しました');
      }
    },
  });

  const handleClose = (open: boolean) => {
    if (!open) {
      form.reset();
      setServerError('');
      setCreatedToken(null);
    }
    onOpenChange(open);
  };

  return (
    <Dialog.Root open={open} onOpenChange={(e) => handleClose(e.open)} /* ... */>
      {/* ... */}
      <Dialog.Body>
        {createdToken ? (
          /* トークン表示部分 — 変更なし */
        ) : (
          {/* form に id を付与し、Dialog.Footer の submit ボタンから form 属性で関連付ける */}
          <form
            id="create-token-form"
            onSubmit={(e) => {
              e.preventDefault();
              e.stopPropagation();
              form.handleSubmit();
            }}
          >
            <form.Field name="name">
              {(field) => (
                <Field.Root invalid={
                  (field.state.meta.isTouched && field.state.meta.errors.length > 0) ||
                  !!serverError
                }>
                  <Field.Label>トークン名</Field.Label>
                  <Input
                    placeholder="例: CI用トークン"
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    onBlur={field.handleBlur}
                    maxLength={100}
                  />
                  {field.state.meta.isTouched && field.state.meta.errors.length > 0 && (
                    <Field.ErrorText>
                      {field.state.meta.errors
                        .map((e) => typeof e === 'string' ? e : (e?.message ?? String(e)))
                        .join(', ')}
                    </Field.ErrorText>
                  )}
                  {serverError && <Field.ErrorText>{serverError}</Field.ErrorText>}
                </Field.Root>
              )}
            </form.Field>
          </form>
        )}
      </Dialog.Body>
      {/* Dialog.Footer は Dialog.Content 直下に配置。submit ボタンは form 属性で紐付け */}
      <Dialog.Footer>
        <form.Subscribe
          selector={(state) => [state.canSubmit, state.isSubmitting] as const}
        >
          {([canSubmit, isSubmitting]) => (
            <HStack>
              <Button variant="outline" onClick={() => handleClose(false)} disabled={isSubmitting}>
                キャンセル
              </Button>
              <Button
                type="submit"
                form="create-token-form"
                colorPalette="blue"
                loading={isSubmitting}
                disabled={!canSubmit}
              >
                作成
              </Button>
            </HStack>
          )}
        </form.Subscribe>
      </Dialog.Footer>
    </Dialog.Root>
  );
}
```

**削減される useState:**
- `name` → `form.Field` が管理
- `error` → `field.state.meta.errors` + `serverError` useState で管理
- `createdToken` → そのまま `useState` で維持（フォームのフィールドではないため）
- `serverError` → `useState` で管理（API エラー専用。TanStack Form にサーバーエラー反映の公式パターンがないため）

### 10. BoardFlow 固有: RevokeTokenDialog について

RevokeTokenDialog は確認ダイアログであり、ユーザー入力フィールドがない。TanStack Form への移行はオーバーエンジニアリングになる可能性がある。

**判断:** RevokeTokenDialog は TanStack Form に移行しない（フォームフィールドが0個の確認ダイアログには不適切）。現状の useState(error) + mutation パターンを維持。

## BoardFlow への示唆

1. **CreateTokenDialog** は TanStack Form への移行対象。useState 3個 → 2個（createdToken + serverError）に削減
2. **RevokeTokenDialog** はフォームフィールドがないため移行対象外
3. zod スキーマを `src/lib/schemas/` などに切り出すと、バリデーションロジックの共有が容易
4. `form.Field` と Chakra UI `Field` の名前衝突は `form.Field` を使えば問題なし
5. `@tanstack/zod-form-adapter` は不要。zod 3.24+ を使えば Standard Schema で直接渡せる
6. 将来フォームが増えた場合にも同じパターンを適用可能

## 採用/不採用判断

| 項目 | 判断 | 理由 |
|---|---|---|
| `@tanstack/react-form` | **採用** | 型安全なフォーム管理、useState削減、zod統合、公式Chakra UI統合サポート |
| `zod` | **採用** | Standard Schema によるバリデーション統合、スキーマ共有 |
| `@tanstack/zod-form-adapter` | **不採用** | zod 3.24+ で不要（Standard Schema 対応済み） |
| `@tanstack/react-form-nextjs` | **不採用** | Server Actions 統合は現段階で不要（フォームは Client Component 内） |
| CreateTokenDialog の移行 | **採用** | useState 削減、バリデーション宣言的管理 |
| RevokeTokenDialog の移行 | **不採用** | フォームフィールドが0個、オーバーエンジニアリング |

## 制約と pitfall

1. **`form.Field` vs Chakra `Field` の名前衝突**: `form.Field` を使えば問題ないが、`useForm` を destructure する場合は rename 必要
2. **バリデーションで transform された値は onSubmit に渡されない**: `onSubmit` で `schema.parse(value)` を明示的に呼ぶ必要がある。ただし BoardFlow では trim のみなので `z.string().trim()` が Standard Schema で機能するか要検証
3. **isSubmitting vs isPending**: TanStack Form の `isSubmitting`（form.handleSubmit 中）と TanStack Query の `isPending`（mutation 中）は同期しているが、使い分けに注意
4. **Server-side エラー表示**: API からのエラーレスポンスをフォームのエラーとして表示するには追加の state 管理が必要になる可能性あり
5. **ダイアログ内フォーム**: `<form>` タグを Dialog.Body 内に配置する場合、Dialog のネイティブボタンとの干渉に注意
6. **Standard Schema の trim()**: `z.string().trim()` は Standard Schema validation で transform として扱われるため、`field.state.value` は trim されない。`onSubmit` の `value` も transform 前の値。submit 時に明示的に trim するか、`onChange` の `handleChange` 内で trim する設計が必要

## 未解決の疑問

1. ~~`z.string().trim()` が Standard Schema validation で正しく動作するか~~
   → 動作するが transform として扱われるため、`onSubmit` の `value` は trim 前の値。明示的に `value.name.trim()` とする必要がある。**解決済み**
2. TanStack Form の `onSubmit` 内で throw された Error がどのように表示されるか（`form.Subscribe` の `errorMap.onSubmit` に反映されるか）— 要実装時検証
3. 将来的に他のフォーム（設定画面等）が追加された場合の共通パターン（schema ファイル分割方針）

## 参照URL

- [TanStack Form v1 公式ドキュメント](https://tanstack.com/form/v1/docs/overview)
- [TanStack Form インストールガイド](https://tanstack.com/form/v1/docs/installation)
- [TanStack Form バリデーションガイド](https://tanstack.com/form/v1/docs/framework/react/guides/validation)
- [TanStack Form UI Libraries ガイド](https://tanstack.com/form/v1/docs/framework/react/guides/ui-libraries)
- [TanStack Form Submission Handling](https://tanstack.com/form/v1/docs/framework/react/guides/submission-handling)
- [TanStack Form SSR/Next.js ガイド](https://tanstack.com/form/v1/docs/framework/react/guides/ssr)
- [TanStack Form Query Integration Example](https://tanstack.com/form/v1/docs/framework/react/examples/query-integration)
- [TanStack Form Standard Schema Example](https://tanstack.com/form/v1/docs/framework/react/examples/standard-schema)
- [@tanstack/react-form npm](https://www.npmjs.com/package/@tanstack/react-form) — v1.29.1
- [zod-form-adapter deprecated (GitHub Issue #1136)](https://github.com/TanStack/form/issues/1136)
- [React 19 互換性確認 (Answer Overflow)](https://www.answeroverflow.com/m/1339176859002343425)
