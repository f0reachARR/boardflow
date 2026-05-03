````markdown
# TanStack Query ベストプラクティスまとめ

対象記事: https://zenn.dev/dragon1208/articles/c3da1a8970fcbd

## 概要

TanStack Queryでは、`useQuery` を安易にカスタムフックでラップするよりも、`queryOptions` を使ってクエリ設定を再利用する設計が推奨される。

特に v5 では、`queryOptions` を使うことで以下のメリットがある。

- 型推論を壊しにくい
- `useQuery` 以外にも再利用できる
- `useSuspenseQuery` や `prefetchQuery` にも使いやすい
- `select` などによる戻り値の型変換にも対応しやすい
- カスタムフックの肥大化を避けられる

---

## よくある問題: `useQuery` を雑にラップする

以下のようなカスタムフックは一見便利に見える。

```ts
export function useInvoice(
  id: number,
  options?: Partial<UseQueryOptions>
) {
  return useQuery({
    queryKey: ["invoice", id],
    queryFn: () => fetchInvoice(id),
    ...options,
  });
}
```

しかし、この書き方には問題がある。

### 問題点

* `UseQueryOptions` の型指定が難しい
* Genericsを省略すると `data` が `unknown` になりやすい
* `select` を使ったときの型推論が壊れやすい
* `useQuery` に強く依存する
* `prefetchQuery` や `useSuspenseQuery` で再利用しにくい

---

## 推奨: `queryOptions` を使う

クエリの設定は、カスタムフックではなく `queryOptions` に切り出す。

```ts
import { queryOptions, useQuery } from "@tanstack/react-query";

type Invoice = {
  id: number;
  amount: number;
};

async function fetchInvoice(id: number): Promise<Invoice> {
  const res = await fetch(`/api/invoices/${id}`);
  return res.json();
}

export function invoiceOptions(id: number) {
  return queryOptions({
    queryKey: ["invoice", id] as const,
    queryFn: () => fetchInvoice(id),
  });
}

export function useInvoice(id: number) {
  return useQuery(invoiceOptions(id));
}
```

---

## `queryOptions` のメリット

### 1. 型推論が保たれる

`queryOptions` を使うと、`queryKey` と `queryFn` の関係をTypeScriptが適切に扱える。

```ts
const query = useQuery(invoiceOptions(invoiceId));

// query.data は Invoice | undefined と推論される
```

---

### 2. 呼び出し側でオプションを合成できる

画面ごとに `staleTime` や `select` を変えたい場合は、呼び出し側で追加する。

```ts
const query = useQuery({
  ...invoiceOptions(invoiceId),
  staleTime: 5_000,
});
```

`select` も自然に使える。

```ts
const query = useQuery({
  ...invoiceOptions(invoiceId),
  select: (invoice) => invoice.amount,
});

// query.data は number | undefined と推論される
```

---

### 3. `useQuery` 以外にも再利用できる

`queryOptions` は単なる設定オブジェクトなので、さまざまな場所で使える。

```ts
useQuery(invoiceOptions(id));

useSuspenseQuery(invoiceOptions(id));

queryClient.prefetchQuery(invoiceOptions(id));
```

カスタムフックに閉じ込めてしまうと、ReactコンポーネントやHookの中でしか使えない。

---

## カスタムフックは不要なのか

カスタムフック自体が悪いわけではない。

ただし、単に `queryKey` や `queryFn` を共有する目的で `useQuery` をラップするのは避けた方がよい。

### カスタムフックを使ってよいケース

* 複数のクエリを組み合わせる
* MutationとQueryをまとめて扱う
* 画面固有の状態管理と組み合わせる
* ドメインロジックをまとめる
* UI側から見て意味のある操作単位にしたい

### 避けたいケース

* `queryKey` と `queryFn` を隠すだけ
* `UseQueryOptions` をそのまま受け取るだけ
* 型推論を壊してまで抽象化する
* `select` や `enabled` などを無理にフックの引数で吸収する

---

## Mutationの場合

Mutationでも同様に、設定を共有したい場合は `mutationOptions` を使う。

```ts
import { mutationOptions, useMutation } from "@tanstack/react-query";

function updateInvoiceOptions() {
  return mutationOptions({
    mutationFn: updateInvoice,
  });
}

const mutation = useMutation({
  ...updateInvoiceOptions(),
  onSuccess: () => {
    // 画面固有の処理
  },
});
```

---

## エラー処理の共通化

全体で共通のエラートーストなどを出したい場合、各 `useQuery` をラップするよりも、`QueryCache` や `MutationCache` のグローバルコールバックに寄せる。

```ts
import {
  QueryClient,
  QueryCache,
  MutationCache,
} from "@tanstack/react-query";

const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (error) => {
      // 共通のエラー処理
    },
  }),
  mutationCache: new MutationCache({
    onError: (error) => {
      // 共通のMutationエラー処理
    },
  }),
});
```

---

## 推奨する設計の流れ

```txt
API関数
  ↓
queryOptions / mutationOptions
  ↓
useQuery / useSuspenseQuery / prefetchQuery / useMutation
  ↓
必要なら薄いカスタムフック
```

---

## ディレクトリ構成例

```txt
src/
  features/
    invoices/
      api/
        fetchInvoice.ts
        updateInvoice.ts
      queries/
        invoiceOptions.ts
      hooks/
        useInvoice.ts
      components/
        InvoiceDetail.tsx
```

---

## 実装例

### API関数

```ts
// features/invoices/api/fetchInvoice.ts

export type Invoice = {
  id: number;
  amount: number;
};

export async function fetchInvoice(id: number): Promise<Invoice> {
  const res = await fetch(`/api/invoices/${id}`);

  if (!res.ok) {
    throw new Error("Failed to fetch invoice");
  }

  return res.json();
}
```

---

### queryOptions

```ts
// features/invoices/queries/invoiceOptions.ts

import { queryOptions } from "@tanstack/react-query";
import { fetchInvoice } from "../api/fetchInvoice";

export function invoiceOptions(id: number) {
  return queryOptions({
    queryKey: ["invoice", id] as const,
    queryFn: () => fetchInvoice(id),
  });
}
```

---

### コンポーネントで使う

```tsx
import { useQuery } from "@tanstack/react-query";
import { invoiceOptions } from "../queries/invoiceOptions";

export function InvoiceDetail({ invoiceId }: { invoiceId: number }) {
  const { data, isLoading, error } = useQuery(invoiceOptions(invoiceId));

  if (isLoading) {
    return <p>Loading...</p>;
  }

  if (error) {
    return <p>Error</p>;
  }

  return (
    <div>
      <p>ID: {data?.id}</p>
      <p>Amount: {data?.amount}</p>
    </div>
  );
}
```

---

### `select` を使う場合

```ts
const amountQuery = useQuery({
  ...invoiceOptions(invoiceId),
  select: (invoice) => invoice.amount,
});

// amountQuery.data は number | undefined
```

---

## ベストプラクティスまとめ

### やるべきこと

* クエリ設定は `queryOptions` に切り出す
* Mutation設定は `mutationOptions` に切り出す
* `queryKey` は一貫した形式で管理する
* オプションは呼び出し側で合成する
* `select` は呼び出し側で指定する
* 共通エラー処理は `QueryCache` / `MutationCache` に寄せる
* カスタムフックは必要最小限にする

### 避けるべきこと

* `useQuery` を何でもカスタムフックで包む
* `UseQueryOptions` を雑に `Partial` で受け取る
* 型推論を壊す抽象化をする
* `select` の型変換を無理にカスタムフック側で吸収する
* グローバルな副作用を各フックに散らばらせる

---

## 結論

TanStack Queryでは、`useQuery` をラップするのではなく、`queryOptions` を使ってクエリ設定を再利用するのがよい。

カスタムフックは、単なる設定の再利用ではなく、複数の処理をまとめる必要がある場合に限定する。

つまり、基本方針は以下の通り。

```txt
設定は queryOptions に置く
利用は useQuery 側で合成する
複雑な処理だけカスタムフックにする
```

