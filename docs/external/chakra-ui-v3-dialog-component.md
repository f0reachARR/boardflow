# Chakra UI v3 Dialog (Modal) コンポーネント

## 要約

Chakra UI v3 では v2 の `Modal` コンポーネントが `Dialog` に改名され、Compound Components パターンに移行した。`<Dialog.Root>` を頂点とし、`Dialog.Trigger`, `Dialog.Backdrop`, `Dialog.Positioner`, `Dialog.Content`, `Dialog.Header`, `Dialog.Body`, `Dialog.Footer`, `Dialog.CloseTrigger` を組み合わせて使う。Alert Dialog は別コンポーネントではなく `role="alertdialog"` prop で対応する。

## 確認した情報

### 基本構造

```tsx
import { Button, CloseButton, Dialog, Portal } from "@chakra-ui/react"

function BasicDialog() {
  return (
    <Dialog.Root>
      <Dialog.Trigger asChild>
        <Button variant="outline">Open Dialog</Button>
      </Dialog.Trigger>
      <Portal>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>Dialog Title</Dialog.Title>
            </Dialog.Header>
            <Dialog.Body>
              <p>Dialog body content here.</p>
            </Dialog.Body>
            <Dialog.Footer>
              <Dialog.ActionTrigger asChild>
                <Button variant="outline">Cancel</Button>
              </Dialog.ActionTrigger>
              <Button>Save</Button>
            </Dialog.Footer>
            <Dialog.CloseTrigger asChild>
              <CloseButton size="sm" />
            </Dialog.CloseTrigger>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  )
}
```

### Controlled Dialog（open / onOpenChange）

```tsx
import { useState } from "react"
import { Button, Dialog, Portal } from "@chakra-ui/react"

function ControlledDialog() {
  const [open, setOpen] = useState(false)

  return (
    <Dialog.Root open={open} onOpenChange={(e) => setOpen(e.open)}>
      <Dialog.Trigger asChild>
        <Button>Open</Button>
      </Dialog.Trigger>
      <Portal>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>Create Token</Dialog.Title>
            </Dialog.Header>
            <Dialog.Body>{/* form content */}</Dialog.Body>
            <Dialog.Footer>
              <Dialog.ActionTrigger asChild>
                <Button variant="outline">Cancel</Button>
              </Dialog.ActionTrigger>
              <Button>Create</Button>
            </Dialog.Footer>
            <Dialog.CloseTrigger />
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  )
}
```

**注意**: `onOpenChange` のコールバック引数は `{ open: boolean }` 型（`OpenChangeDetails`）。

### Alert Dialog（revoke 確認用）

v3 では独立した `AlertDialog` コンポーネントは**廃止**。`Dialog.Root` に `role="alertdialog"` を付けるだけで対応する。

```tsx
<Dialog.Root role="alertdialog" open={open} onOpenChange={(e) => onOpenChange(e.open)}>
  <Portal>
    <Dialog.Backdrop />
    <Dialog.Positioner>
      <Dialog.Content>
        <Dialog.Header>
          <Dialog.Title>確認</Dialog.Title>
        </Dialog.Header>
        <Dialog.Body>
          <Text>本当に削除しますか？</Text>
        </Dialog.Body>
        <Dialog.Footer>
          <Button variant="outline" onClick={() => onOpenChange(false)}>キャンセル</Button>
          <Button colorPalette="red" onClick={onConfirm}>削除</Button>
        </Dialog.Footer>
      </Dialog.Content>
    </Dialog.Positioner>
  </Portal>
</Dialog.Root>
```

### Close 防止 props

- `closeOnInteractOutside={false}` — backdrop クリックでの close を防止
- `closeOnEscape={false}` — ESC キーでの close を防止
- これらは動的に切り替え可能（例: loading 中のみ false）

## v2 → v3 移行メモ

| v2 | v3 |
|---|---|
| `<Modal>` | `<Dialog.Root>` |
| `<ModalOverlay>` | `<Dialog.Backdrop>` (Portal 内) |
| `<ModalContent>` | `<Dialog.Positioner>` + `<Dialog.Content>` |
| `<ModalHeader>` | `<Dialog.Header>` |
| `<ModalBody>` | `<Dialog.Body>` |
| `<ModalFooter>` | `<Dialog.Footer>` |
| `<ModalCloseButton>` | `<Dialog.CloseTrigger>` |
| `isOpen` | `open` |
| `onClose` | `onOpenChange={(e) => ...}` |
| `<AlertDialog>` | `<Dialog.Root role="alertdialog">` |

## 参照

- https://chakra-ui.com/docs/components/dialog
- https://chakra-ui.com/docs/get-started/migration
