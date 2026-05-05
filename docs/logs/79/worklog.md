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

- <https://tanstack.com/form/v1/docs/overview>
- <https://tanstack.com/form/v1/docs/installation>
- <https://tanstack.com/form/v1/docs/framework/react/guides/validation>
- <https://tanstack.com/form/v1/docs/framework/react/guides/ui-libraries>
- <https://tanstack.com/form/v1/docs/framework/react/guides/submission-handling>
- <https://tanstack.com/form/v1/docs/framework/react/examples/query-integration>
- <https://www.npmjs.com/package/@tanstack/react-form>
- <https://github.com/TanStack/form/issues/1136>

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

---

## ドキュメント確認結果（2026-05-05 docs agent）

### 総評

- Issue #79 の実装本体は [boardflow/src/components/tokens/create-token-dialog.tsx](boardflow/src/components/tokens/create-token-dialog.tsx#L34) と [boardflow/src/components/tokens/create-token-dialog.tsx](boardflow/src/components/tokens/create-token-dialog.tsx#L53) を見る限り、TanStack Form + zod への移行内容と整合している。
- ただし、research 成果物の [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L317) と作業ログの既存レビュー/実装記録には、現実装と食い違う記述が残っている。
- [docs/technology.md](docs/technology.md#L52) は repo 全体の粗い技術方針の粒度で書かれており、今回の変更だけでは必須更新とは言い切れない。一方で frontend の採用スタック表である [docs/frontend/summary.md](docs/frontend/summary.md#L17) には、必要なら Form/Validation の行を追加する余地がある。

### PR 判定

- `docs_ready: false`

### 必須修正

1. [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L317) の BoardFlow 固有サンプルを現実装に合わせる。

- [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L359) では `z.string().trim()` を使ったスキーマに対して submit 側で `value.name` をそのまま送っており、同ファイル内の transform 注意書き [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L470) と矛盾している。
- 実装は [boardflow/src/components/tokens/create-token-dialog.tsx](boardflow/src/components/tokens/create-token-dialog.tsx#L53) の通り `value.name.trim()` を送っているため、research メモ側のサンプルもそれに合わせる必要がある。
- 同サンプルの `Dialog.Footer` 構成も、実装の [boardflow/src/components/tokens/create-token-dialog.tsx](boardflow/src/components/tokens/create-token-dialog.tsx#L115) と [boardflow/src/components/tokens/create-token-dialog.tsx](boardflow/src/components/tokens/create-token-dialog.tsx#L176) のように `form` 属性で submit を関連付ける形へ揃えるべき。

2. [docs/logs/79/worklog.md](docs/logs/79/worklog.md#L395) 以降の既存レビュー結果を、現実装に合わせて訂正する。

- [docs/logs/79/worklog.md](docs/logs/79/worklog.md#L395) は `Dialog.Footer` が `form` 配下にあると指摘しているが、現実装はそうなっていない。
- [docs/logs/79/worklog.md](docs/logs/79/worklog.md#L487) の実装記録も `Dialog.Footer` を `form` 内に置いたと書いており、コード実態と不一致。
- [docs/logs/79/worklog.md](docs/logs/79/worklog.md#L499) の「external ドキュメントは変更不要」という判断も、上記不整合があるため成立していない。

### 任意改善

1. [docs/frontend/summary.md](docs/frontend/summary.md#L17) の採用スタック表に、Form/Validation として TanStack Form + zod を追加すると、frontend 標準の見通しがよくなる。
2. [docs/technology.md](docs/technology.md#L52) は現状の粒度でも問題ないが、repo 全体の技術スタック一覧をより網羅的にしたい場合だけ frontend 補助ライブラリの記載を検討する。

### 不整合のあるドキュメント

- [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L317)
- [docs/logs/79/worklog.md](docs/logs/79/worklog.md#L395)

### 不足しているドキュメント

- 必須の追記先はない。
- 任意で [docs/frontend/summary.md](docs/frontend/summary.md#L17) に Form/Validation 行を追加するとよい。

### 外部調査メモに関する指摘

- [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L470) と [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L474) の注意書き自体は妥当だが、BoardFlow 固有サンプル側がその結論を反映できていない。
- research メモは「trim は submit 時に明示処理が必要」という判断を出せているので、サンプルコードだけを現実装ベースに直せば整合する。

### PR本文案に対する確認

- 概要と変更内容の方向性は妥当。
- ただし docs 修正を入れる前提なら、更新ドキュメントとして research メモと worklog を追記したことを本文に含めた方が整合する。
- テスト欄は実際に確認したコマンドだけを書くべきで、`pnpm lint` ではなく `pnpm lint --write --unsafe` を実行事実として書く方が正確。

### 更新した作業ログパス

- `docs/logs/79/worklog.md`

---

## レビュー結果（2026-05-05 review agent）

### 総評

- CreateTokenDialog の TanStack Form + zod への移行は、コード・research・実装差分・検証結果の整合が取れている。
- `pnpm typecheck`, `pnpm lint`, `pnpm build` は現ブランチで再確認できた。
- RevokeTokenDialog は Issue 79 の対象外として未変更を確認した。
- ブロッカーは見当たらないため、PR 作成は可能と判断する。

### PR 判定

- `pr_ready: true`

### 指摘事項

1. **warning**: `Dialog.Footer` が Chakra UI のドキュメント例と異なり、`Dialog.Content` の直下ではなく `form` 配下にネストされている。

- 対象: `boardflow/src/components/tokens/create-token-dialog.tsx` の `form` 配下にあるフッター
- 参照: Chakra UI の Dialog 例は `Dialog.Content > Dialog.Header / Dialog.Body / Dialog.Footer` 構造を前提にしている
- 影響: 現状でビルドや型は通るが、将来の Chakra UI 更新時にレイアウトや余白、アクセシビリティ上の前提が崩れる可能性がある
- 対応案: `form` を `Dialog.Body` と `Dialog.Footer` の両方を包む位置へ移す、または `form` に `id` を付与して `Button` 側の `form` 属性で submit を紐付ける

1. **info**: 作業ログの受け入れ条件 3 が、実装内容と不一致になっている。

- 対象: `docs/logs/79/worklog.md` の受け入れ条件
- 現状: `createdToken` のみ残すと記載されているが、実装と Issue 要約では `createdToken` と `serverError` を残す構成
- 影響: 実装評価基準の読み手に誤解を与える
- 対応案: 受け入れ条件 3 を実装実態に合わせて修正する

### 受け入れ条件評価

1. `@tanstack/react-form` と `zod` の追加: 満たす
2. CreateTokenDialog の `useForm` + zod 移行: 満たす
3. `name`, `error` の `useState` 削減と `createdToken` / `serverError` 残置: 実装は満たす
4. `field.state.meta.errors` によるフィールドエラー表示: 満たす
5. サーバーエラー表示: 満たす
6. close 時の `form.reset()`: 満たす
7. `pnpm typecheck`, `pnpm lint`, `pnpm build`: 満たす
8. RevokeTokenDialog 非変更: 満たす

### 必須修正

- なし

### 任意改善

- `Dialog.Footer` を Chakra UI の推奨構造に寄せる
- `catch (err: unknown)` の API エラー取り出しを型ガード化して再利用可能にする

### テスト不足

- フロントエンドの自動 UI テストがないため、以下はビルド成功だけでは担保できない
- 空 submit 時にフィールドエラーが表示されること
- API エラー後に再入力して再 submit できること
- close 時に `form.reset()` と `createdToken` / `serverError` のリセットが効くこと

### ドキュメント確認

- `docs/external/tanstack-form-chakra-integration.md`: 実装方針と整合
- `docs/spec.md`: 本 Issue は UI 実装詳細のため仕様変更不要
- `README.md`: 更新不要
- `docs/logs/79/worklog.md`: 受け入れ条件 3 の記述だけ実装実態と不一致

### plan / research / docs との不整合

- plan/worklog 上の受け入れ条件 3 が古い記述のまま残っている
- research で触れていた Chakra UI の構造例と、実装上の `Dialog.Footer` 配置が一致していない

### 残リスク

- Dialog 構造の逸脱が Chakra UI 将来更新で問題化する可能性
- UI 操作系の回帰が自動テストで捕捉されない

### 更新した作業ログパス

- `docs/logs/79/worklog.md`

---

## 実装結果（2026-05-05 impl agent）

### 実装内容

ブランチ `feature/79-tanstack-form` で CreateTokenDialog を TanStack Form + zod に移行した。

**変更ファイル:**

| ファイル | 変更内容 |
|---|---|
| `boardflow/package.json` | `@tanstack/react-form` v1.29.1, `zod` v4.4.3 追加 |
| `boardflow/pnpm-lock.yaml` | ロックファイル更新 |
| `boardflow/src/components/tokens/create-token-dialog.tsx` | TanStack Form + zod リファクタリング |

**主な変更点:**

1. `useState(name)`, `useState(error)` を削除 → `useForm` + `form.Field` に置き換え
2. `useState(serverError)` を追加（APIエラー専用）
3. `mutate` → `mutateAsync`、`onSuccess`/`onError` コールバック → `onSubmit` 内 try/catch
4. zod スキーマ `createTokenSchema` でクライアントバリデーション（`validators.onChange`）
5. `form.Field` render props → JSX children 構文（biome `noChildrenProp` 対応）
6. `form.Subscribe` で `canSubmit`/`isSubmitting` を監視してボタン制御
7. `form.reset()` でダイアログ close 時のリセット
8. `form.state.isSubmitting` で `closeOnInteractOutside`/`closeOnEscape`/`CloseTrigger` 制御

**実装時の発見・対処:**

- **zod v4**: `pnpm add zod` で v4.4.3 がインストールされた（計画時は v3.24+ を想定）。Standard Schema サポートは互換で問題なし
- **`field.state.meta.errors` の型**: 要素が `undefined` になりうる（`ValidationError[]` の要素型）。`typeof e === 'string' ? e : (e as { message?: string })?.message ?? String(e)` で安全に処理
- **biome `noChildrenProp`**: `children` prop パターンは lint エラー。JSX children 構文 `{(field) => (...)}` に変換して解消
- **Dialog.Footer の配置**: `<form>` タグ内にフッターを含める必要があるため、フォーム表示時は `form` 内に `Dialog.Footer` を配置し、トークン作成後のフッターは `Dialog.Body` の外に別途配置

### テスト結果

| チェック | 結果 |
|---|---|
| `pnpm typecheck` | パス |
| `pnpm lint --write --unsafe` | パス（biome 0 errors） |
| `pnpm build` | パス（Next.js 16.2.4 Turbopack） |

**注**: 本プロジェクトにはフロントエンドのユニットテスト（Jest/Vitest 等）が未導入のため、TDD の red-green は typecheck + lint + build で代替。

### ドキュメント確認

- `docs/external/tanstack-form-chakra-integration.md` — research agent が作成済み、変更不要
- `docs/technology.md` — TanStack Form / zod の追記は今回スコープ外（既存にフロントエンドライブラリの詳細列挙なし）
- `RevokeTokenDialog` — 変更なし（確認済み）

### 残リスク

1. **フロントエンド E2E / ユニットテスト未整備**: CreateTokenDialog の動作は手動確認が必要。将来的に Vitest + Testing Library の導入を推奨
2. **zod v4 と TanStack Form の長期互換性**: zod v4 は Standard Schema 対応だが、TanStack Form 側の公式テストが zod v3 ベースの可能性あり。問題があれば `pnpm add zod@3` でダウングレード可能
3. **サーバーエラーの型安全性**: `catch (err: unknown)` → `as { error?: { message?: string } }` のキャストは型安全ではない。openapi-react-query のエラー型定義が改善されれば型ガードに置き換え可能

### 更新した作業ログパス

`docs/logs/79/worklog.md`

---

## レビュー指摘修正 + ドキュメント修正（2026-05-05 orchestrator）

### 修正内容

1. **Dialog.Footer 構造修正**（review warning 対応）:
   - `Dialog.Footer` を `<form>` 内から `Dialog.Content` 直下に移動
   - `<form>` に `id="create-token-form"` を付与
   - submit ボタンに `form="create-token-form"` 属性を追加して関連付け
   - Chakra UI の推奨構造（Dialog.Content > Header/Body/Footer）に準拠

2. **docs/external/tanstack-form-chakra-integration.md 修正**:
   - BoardFlow 固有サンプル（セクション9）を実装に合わせて更新
   - `z.string().trim()` → `z.string()` + onSubmit 内 `value.name.trim()` に修正
   - `serverError` useState の追加
   - form id + form 属性パターンでの Dialog.Footer 分離を反映
   - useState 削減の記述を「3個 → 2個（createdToken + serverError）」に修正

3. **docs/frontend/summary.md 更新**:
   - 採用スタック表に「フォーム / バリデーション | TanStack Form + zod」行を追加

### 検証結果

- `pnpm typecheck` — パス
- `pnpm lint --write --unsafe` — パス
- `pnpm build` — パス

---

## ドキュメント再確認結果（2026-05-05 docs agent, follow-up）

### 対象 Issue

- Issue #79: TanStack Formへのフォームリファクタリング

### 総評

- [docs/external/tanstack-form-chakra-integration.md](docs/external/tanstack-form-chakra-integration.md#L334) の BoardFlow 固有サンプルは、[boardflow/src/components/tokens/create-token-dialog.tsx](boardflow/src/components/tokens/create-token-dialog.tsx#L16) と [boardflow/src/components/tokens/create-token-dialog.tsx](boardflow/src/components/tokens/create-token-dialog.tsx#L36) 以降の実装に整合している。
- `z.string().trim()` をサンプルから除外し、submit 時に `value.name.trim()` を使う点、`serverError` を別 state で管理する点、`form id` + `form` 属性で `Dialog.Footer` の submit を関連付ける点が実装どおりに反映されている。
- [docs/frontend/summary.md](docs/frontend/summary.md#L18) の「フォーム / バリデーション | TanStack Form + zod」追加も、frontend 採用スタックの粒度として適切。
- 前回の必須修正 2 件と任意改善 1 件は、現時点ではいずれも対応済みと判断する。

### PR 判定

- `docs_ready: true`

### 必須修正

- なし

### 任意改善

- なし

### 不整合のあるドキュメント

- なし

### 不足しているドキュメント

- なし

### 外部調査メモに関する指摘

- 以前の指摘対象だった BoardFlow 固有サンプルは解消済み。本文中の trim に関する注意書きとサンプルコードの採用判断も一致している。

### 残リスク

- worklog には過去レビュー時点の `docs_ready: false` 判定が履歴として残っているが、後続の修正記録と今回の再確認結果が時系列で追記されているため、運用上の矛盾とはみなさない。

### 更新した作業ログパス

- `docs/logs/79/worklog.md`

---

## PR作成結果（2026-05-05 pr agent）

### PR情報

- **PR URL**: <https://github.com/f0reachARR/boardflow/pull/87>
- **タイトル**: feat(frontend): migrate CreateTokenDialog to TanStack Form + zod
- **ブランチ**: `feature/79-tanstack-form` → `main`
- **コミット**: 3件（bd04ecb, 146c508, 0097e54）

### PR作成前確認事項

| 確認項目 | 結果 |
|---|---|
| review: pr_ready | true |
| docs: docs_ready | true |
| 未コミット変更 | なし |
| リモートプッシュ | 最新（Everything up-to-date） |
| pnpm typecheck | パス |
| pnpm lint --write --unsafe | パス (0 errors) |
| pnpm build | パス (Turbopack) |

### 残リスク

- 将来のフォーム追加時のスキーマ分割方針は未定（現時点ではフォームが1つのみのため先送り）
