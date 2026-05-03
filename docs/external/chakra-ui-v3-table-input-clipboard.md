# Chakra UI v3 Table / Input / Field / Clipboard コンポーネント

## 要約

BoardFlow の Token 管理 UI で使用する Chakra UI v3 コンポーネントの使用法メモ。Table は既存コードで使用済みパターン、Field + Input はフォーム入力、Clipboard はトークンのコピー機能で使用。

## Table

既存の repositories/[repositoryId]/page.tsx と同じ Compound Components パターン。

```tsx
import { Table } from "@chakra-ui/react"

<Table.Root size="sm" variant="outline">
  <Table.Header>
    <Table.Row>
      <Table.ColumnHeader>Name</Table.ColumnHeader>
      <Table.ColumnHeader>Status</Table.ColumnHeader>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {items.map((item) => (
      <Table.Row key={item.id}>
        <Table.Cell>{item.name}</Table.Cell>
        <Table.Cell>{item.status}</Table.Cell>
      </Table.Row>
    ))}
  </Table.Body>
</Table.Root>
```

## Field + Input

v2 の `FormControl` → v3 の `Field.Root`。

```tsx
import { Field, Input } from "@chakra-ui/react"

<Field.Root invalid={!!error}>
  <Field.Label>トークン名</Field.Label>
  <Input
    placeholder="例: CI用トークン"
    value={name}
    onChange={(e) => setName(e.target.value)}
    maxLength={100}
  />
  {error && <Field.ErrorText>{error}</Field.ErrorText>}
</Field.Root>
```

### v2 → v3 変更点

| v2 | v3 |
|---|---|
| `<FormControl>` | `<Field.Root>` |
| `<FormLabel>` | `<Field.Label>` |
| `<FormErrorMessage>` | `<Field.ErrorText>` |
| `<FormHelperText>` | `<Field.HelperText>` |
| `isInvalid` | `invalid` |
| `isRequired` | `required` |
| `isDisabled` | `disabled` |

## Clipboard（v3 新規）

v3 で新規追加。`navigator.clipboard` API を使用（HTTPS/localhost 必須）。

```tsx
import { Clipboard, Button, HStack } from "@chakra-ui/react"

<Clipboard.Root value={tokenValue}>
  <Clipboard.Label>トークン</Clipboard.Label>
  <HStack>
    <Clipboard.Input />
    <Clipboard.Trigger asChild>
      <Button size="sm" variant="outline">コピー</Button>
    </Clipboard.Trigger>
  </HStack>
</Clipboard.Root>
```

- `value` prop でコピー対象テキストを指定
- `Clipboard.Input` は読み取り専用の表示用入力欄
- `Clipboard.Trigger` がクリック時にコピーを実行

## Button

```tsx
import { Button } from "@chakra-ui/react"

// v3 では colorScheme → colorPalette
<Button colorPalette="blue" size="sm">作成</Button>
<Button colorPalette="red" variant="outline" size="xs">Revoke</Button>

// loading state
<Button loading={isLoading} disabled={!isValid}>Submit</Button>
```

## 参照

- https://chakra-ui.com/docs/components/table
- https://chakra-ui.com/docs/components/input
- https://chakra-ui.com/docs/components/field
- https://chakra-ui.com/docs/components/clipboard
