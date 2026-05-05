# Issue #79: TanStack Formへのフォームリファクタリング — 作業ログ

## Issue までの経緯

- ユーザー要望8: すべてのフォームをTanStack Formに移行
- Issue #78（TanStack Query mutation 移行）はマージ済み
- 対象フォーム: CreateTokenDialog（入力フィールドあり）、RevokeTokenDialog（確認ダイアログのみ）

## ユーザー要望

- フォームの状態管理をTanStack Formに統一
- useState 手動管理からの脱却
- zod バリデーション統合
- TanStack Query mutation との連携

## 初期調査（Issue作成時）

- 現在のフォーム: Token作成ダイアログ (`create-token-dialog.tsx`) で `useState` による手動管理
- バリデーションは手動（文字列長チェック等）
- ローディング/エラー状態も `useState` で個別管理

## 外部調査結果（2026-05-05 research agent）

### 調査トピックと結果

1. **TanStack Form v1 基本構造** — 公式ドキュメントから確認完了
   - `useForm`, `form.Field`, `validators`, `onSubmit` の基本パターン把握
   - render props パターンで UI ライブラリと統合

2. **zod バリデーション統合** — Standard Schema（zod 3.24+）で直接統合可能
   - `@tanstack/zod-form-adapter` は廃止済み（GitHub Issue #1136）
   - zod スキーマを `validators.onChange` に直接渡せる
   - **注意**: `z.string().trim()` は transform 扱い。`onSubmit` の value は trim 前

3. **TanStack Query mutation 連携** — 公式パターン確認
   - `onSubmit` 内で `mutateAsync` を呼ぶ
   - `formApi.reset()` でリセット
   - `form.state.isSubmitting` で loading 状態を管理

4. **Chakra UI v3 統合** — 公式 UI Libraries ガイドに例あり
   - Input: `value={field.state.value}`, `onChange={(e) => field.handleChange(e.target.value)}`
   - Checkbox: `onCheckedChange={(details) => handleChange(!!details.checked)}`
   - `form.Field`（TanStack）と `Field`（Chakra UI）の名前衝突なし（`form.Field` を使えば OK）

5. **React 19 / Next.js 互換性** — 確認済み
   - React 19 互換（公式 examples が React 19 使用）
   - Server Actions は `@tanstack/react-form-nextjs` だが今回は Client Component のみのため不要

6. **パッケージ情報**
   - `@tanstack/react-form` v1.29.1（最新）
   - `zod`（新規インストール必要、プロジェクト未導入）

### 移行判断

| フォーム | 判断 | 理由 |
|---|---|---|
| CreateTokenDialog | **移行する** | フォームフィールドあり、useState 3→1 に削減可能 |
| RevokeTokenDialog | **移行しない** | フォームフィールドが0個、確認ダイアログにはオーバーエンジニアリング |

### 成果物

- `docs/external/tanstack-form-chakra-integration.md` — 調査メモ（コード例・移行パターン付き）

### 参照URL

- https://tanstack.com/form/v1/docs/overview
- https://tanstack.com/form/v1/docs/installation
- https://tanstack.com/form/v1/docs/framework/react/guides/validation
- https://tanstack.com/form/v1/docs/framework/react/guides/ui-libraries
- https://tanstack.com/form/v1/docs/framework/react/guides/submission-handling
- https://tanstack.com/form/v1/docs/framework/react/examples/query-integration
- https://www.npmjs.com/package/@tanstack/react-form
- https://github.com/TanStack/form/issues/1136

## 結論ステータス

**`implementation_required`**

## 後続エージェントへの注意点

1. `pnpm add @tanstack/react-form zod` でインストール（`@tanstack/zod-form-adapter` は不要）
2. CreateTokenDialog のみ移行対象。RevokeTokenDialog は現状維持
3. `z.string().trim()` は transform — `onSubmit` の value.name に対して明示的に `.trim()` が必要
4. `form.Field`（TanStack）と `Field`（Chakra UI）の名前衝突は `form.Field` を使えば解決
5. `createdToken` は useState で維持（フォームフィールドではないため）
6. サーバーエラー表示パターンは実装時に検証が必要

## 残リスク

1. `z.string().trim()` の Standard Schema での挙動 — 実装時に動作検証が必要
2. TanStack Form `onSubmit` 内で throw された Error の表示挙動 — 実装時に検証が必要
3. 将来のフォーム追加時の schema 分割方針は未確定（現時点ではフォームが1つのみのため先送り可）

---

## 実装計画（2026-05-05 plan agent）

### 目的

CreateTokenDialog の useState 手動フォーム管理を TanStack Form + zod に移行し、宣言的バリデーション・型安全なフォーム状態管理を実現する。

### 非目的

- RevokeTokenDialog の変更（フォームフィールド0個のため対象外）
- 新しい UI コンポーネントの追加やデザイン変更
- スキーマ定義の別ファイル切り出し（フォーム1つのみのため現時点では不要）
- Server Actions / SSR フォーム統合

### 受け入れ条件

1. `@tanstack/react-form` と `zod` が `package.json` に追加されている
2. CreateTokenDialog が `useForm` + zod バリデーションで動作する
3. `useState` が `name`, `error` の2つ削減され、`createdToken` のみ残っている
4. バリデーションエラーがフィールドレベルで表示される（`field.state.meta.errors`）
5. サーバーエラー（API エラー）が適切に表示される
6. ダイアログ close 時に `form.reset()` でフォームがリセットされる
7. `pnpm typecheck`, `pnpm lint`, `pnpm build` がすべてパスする
8. RevokeTokenDialog は変更されていない

### 詳細要件

#### フォーム状態管理
- `useState('name')` → `form.Field` name フィールドに移行
- `useState('error')` → `field.state.meta.errors` + サーバーエラー用 `useState` に統合
  - クライアントバリデーションエラー: `field.state.meta.errors` で表示
  - サーバーエラー（API失敗時）: `onSubmit` 内 try/catch → useState で管理（TanStack Form にはサーバーエラー反映の公式パターンがないため）
- `useState('createdToken')` → 変更なし（フォームフィールドではない）

#### バリデーション
- zod スキーマ: `z.object({ name: z.string().min(1, '名前は1文字以上で入力してください').max(100, '名前は100文字以内で入力してください') })`
- `validators.onChange` にスキーマを渡す
- **trim は `z.string().trim()` を使わない**。Standard Schema では transform の結果が `onSubmit` の value に反映されない可能性がある。代わりに `onSubmit` 内で `value.name.trim()` を行う（現行コードと同じパターン）

#### Mutation 連携
- `mutate` → `mutateAsync` に変更
- mutation の `onSuccess`/`onError` コールバックを削除し、`onSubmit` 内の try/catch に統合
- `form.state.isSubmitting` でローディング状態を管理

#### Chakra UI 統合
- `form.Field`（TanStack）と `Field`（Chakra UI）は衝突しない（`form.Field` はインスタンスメソッド）
- `Field.Root` の `invalid` prop: `field.state.meta.isTouched && field.state.meta.errors.length > 0`
- エラーテキスト: `field.state.meta.isTouched` の場合のみ表示

#### ダイアログ制御
- `closeOnInteractOutside` / `closeOnEscape`: `form.state.isSubmitting` を参照
- 作成ボタン: `form.Subscribe` で `canSubmit`/`isSubmitting` を監視
- close 時: `form.reset()` + `setCreatedToken(null)`

### 影響範囲

| ファイル | 変更内容 |
|---|---|
| `boardflow/package.json` | `@tanstack/react-form`, `zod` 追加 |
| `boardflow/pnpm-lock.yaml` | パッケージロック更新（自動） |
| `boardflow/src/components/tokens/create-token-dialog.tsx` | TanStack Form + zod へのリファクタリング |

**変更しないファイル:**
- `boardflow/src/components/tokens/revoke-token-dialog.tsx`
- その他すべてのファイル

### 設計方針

#### CreateTokenDialog 移行後の構造

```tsx
'use client';

import { useForm } from '@tanstack/react-form';
import { z } from 'zod';
import {
  Box, Button, Clipboard, Dialog, Field, HStack, Input, Portal, Text,
} from '@chakra-ui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { $api } from '@/lib/api/react-query';

const createTokenSchema = z.object({
  name: z.string()
    .min(1, '名前は1文字以上で入力してください')
    .max(100, '名前は100文字以内で入力してください'),
});

interface Props {
  repositoryId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CreateTokenDialog({ repositoryId, open, onOpenChange }: Props) {
  const [createdToken, setCreatedToken] = useState<string | null>(null);
  const [serverError, setServerError] = useState('');
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
          body: { name: value.name.trim() },
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

  // JSX: form.Field, form.Subscribe を使った宣言的 UI
}
```

#### 主な変更ポイント

1. **import 変更**: `useForm` (from `@tanstack/react-form`), `z` (from `zod`) を追加
2. **useState 削減**: `name`, `error` → 削除。`serverError`（API エラー専用）と `createdToken` の2つに
3. **mutation 変更**: `mutate` → `mutateAsync`、コールバック削除
4. **フォーム入力部分**: `<form>` タグで囲み、`form.Field` render props で Input をバインド
5. **エラー表示**: クライアントエラーは `field.state.meta.errors`、サーバーエラーは `serverError` state
6. **ボタン制御**: `form.Subscribe` で `canSubmit`/`isSubmitting` を監視

### 実装ステップ

#### Step 1: ブランチ作成
```bash
git checkout main && git pull origin main
git checkout -b feature/79-tanstack-form
```

#### Step 2: パッケージインストール
```bash
cd boardflow && pnpm add @tanstack/react-form zod
```
- `@tanstack/zod-form-adapter` は**インストールしない**（zod 3.24+ Standard Schema 対応）
- `@tanstack/react-form-nextjs` も**インストールしない**（Server Actions 不要）

#### Step 3: CreateTokenDialog のリファクタリング

**ファイル**: `boardflow/src/components/tokens/create-token-dialog.tsx`

**3a. import 更新**
- 追加: `import { useForm } from '@tanstack/react-form'`, `import { z } from 'zod'`
- 削除なし（`useState` は `createdToken` と `serverError` 用に残す）

**3b. zod スキーマ定義**
- コンポーネント外に `createTokenSchema` を定義
- `z.string().min(1).max(100)` で日本語エラーメッセージ付き
- `z.string().trim()` は使わない（Standard Schema の transform 挙動が未検証のため）

**3c. useState 変更**
- 削除: `const [name, setName] = useState('')`
- 削除: `const [error, setError] = useState('')`
- 追加: `const [serverError, setServerError] = useState('')`
- 維持: `const [createdToken, setCreatedToken] = useState<string | null>(null)`

**3d. mutation 変更**
- `const { mutate, isPending }` → `const { mutateAsync }`
- `onSuccess`/`onError` コールバックを削除

**3e. useForm 追加**
- `defaultValues: { name: '' }`
- `validators: { onChange: createTokenSchema }`
- `onSubmit`: `mutateAsync` を呼び、成功時 `setCreatedToken`、失敗時 `setServerError`

**3f. handleCreate 削除**
- `handleCreate` 関数を削除（`form.onSubmit` に統合済み）

**3g. handleClose 変更**
- `setName('')`/`setError('')` → `form.reset()`/`setServerError('')`

**3h. JSX 変更**

- フォーム入力部分を `<form onSubmit={...}>` で囲む
- `<Input>` を `<form.Field name="name">` の render props 内に配置
- `Field.Root` の `invalid` を `field.state.meta.isTouched && field.state.meta.errors.length > 0` に変更
- エラー表示: `field.state.meta.errors` + `serverError`
- `closeOnInteractOutside`/`closeOnEscape`: `!createdToken && !form.state.isSubmitting`
- 作成ボタン: `form.Subscribe` で `canSubmit`/`isSubmitting` を監視
- キャンセルボタン: `disabled={form.state.isSubmitting}`

**注意点:**
- `form.Field` render props の `children` は関数（biome が lint エラーを出さないか確認）
- `field.state.meta.errors` は `ValidationError[]` 型。`join(', ')` で文字列化
- `form.Subscribe` の `selector` は配列を返す。型推論が正しく効くか確認

#### Step 4: 検証

```bash
cd boardflow
pnpm typecheck   # TypeScript の型チェック
pnpm lint        # biome lint
pnpm build       # Next.js ビルド
```

- 3つすべてがパスすることを確認
- lint 違反があれば `pnpm lint --write` で自動修正

#### Step 5: コミット & プッシュ

```bash
git add -A
git commit -m "feat(frontend): migrate CreateTokenDialog to TanStack Form + zod

- Replace useState(name, error) with useForm + zod schema validation
- Use mutateAsync in onSubmit instead of mutate with callbacks  
- Declarative field validation with field.state.meta.errors
- Keep createdToken as useState (not a form field)
- Add @tanstack/react-form and zod dependencies

Closes #79"
git push origin feature/79-tanstack-form
```

### テスト観点

| 観点 | 確認方法 |
|---|---|
| 型安全性 | `pnpm typecheck` パス |
| lint | `pnpm lint` パス |
| ビルド | `pnpm build` パス |
| バリデーション: 空文字 | 名前未入力で「作成」ボタンが disabled |
| バリデーション: 100文字超 | 100文字超入力でエラーメッセージ表示 |
| 正常系: トークン作成 | 名前入力→作成→トークン表示画面に遷移 |
| 異常系: API エラー | サーバーエラー時にエラーメッセージ表示 |
| ダイアログ close | close 時にフォームがリセットされる |
| ローディング状態 | 作成中にボタンが loading 表示、ダイアログが閉じない |
| RevokeTokenDialog | 変更されていないこと |

### ドキュメント更新対象

| ドキュメント | 更新内容 |
|---|---|
| `docs/logs/79/worklog.md` | 実装計画（本セクション）、実装後に結果追記 |

**更新不要:**
- `docs/spec.md` — フォームライブラリの選択は仕様変更ではない
- `docs/frontend/` — 技術選定ドキュメントがあれば更新するが、現在フォーム関連の記述なし
- `docs/technology.md` — TanStack Form / zod を追記する場合は実装完了後に判断

### 実装要否

**`implementation_required`**

### 未解決の疑問

なし。research agent の調査が十分であり、ユーザー判断が必要な項目はない。

### 更新した作業ログパス

`docs/logs/79/worklog.md`
