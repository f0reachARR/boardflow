"use client"

import { useState } from "react"
import { Box, Button, Dialog, Field, HStack, Input, Portal, Text, Clipboard } from "@chakra-ui/react"
import { apiClient } from "@/lib/api/client"

interface Props {
  repositoryId: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onCreated: () => void
}

export function CreateTokenDialog({ repositoryId, open, onOpenChange, onCreated }: Props) {
  const [name, setName] = useState("")
  const [error, setError] = useState("")
  const [loading, setLoading] = useState(false)
  const [createdToken, setCreatedToken] = useState<string | null>(null)

  const handleCreate = async () => {
    const trimmed = name.trim()
    if (trimmed.length < 1 || trimmed.length > 100) {
      setError("名前は1〜100文字で入力してください")
      return
    }
    setError("")
    setLoading(true)
    const { data, error: apiError } = await apiClient.POST(
      "/api/v1/repositories/{github_repository_id}/api-tokens",
      {
        params: { path: { github_repository_id: repositoryId } },
        body: { name: trimmed },
      }
    )
    setLoading(false)
    if (apiError) {
      setError(apiError.error.message)
      return
    }
    setCreatedToken(data!.token)
  }

  const handleClose = (open: boolean) => {
    if (!open) {
      if (createdToken) {
        onCreated()
      }
      setName("")
      setError("")
      setCreatedToken(null)
    }
    onOpenChange(open)
  }

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(e) => handleClose(e.open)}
      closeOnInteractOutside={!createdToken}
      closeOnEscape={!createdToken}
    >
      <Portal>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>
                {createdToken ? "トークンが作成されました" : "新しいAPIトークンを作成"}
              </Dialog.Title>
            </Dialog.Header>
            <Dialog.Body>
              {createdToken ? (
                <Box>
                  <Text fontWeight="bold" color="orange.600" mb={3}>
                    この画面を閉じるとトークンは二度と表示されません。
                  </Text>
                  <Clipboard.Root value={createdToken}>
                    <Clipboard.Label>トークン</Clipboard.Label>
                    <HStack>
                      <Clipboard.Input />
                      <Clipboard.Trigger asChild>
                        <Button size="sm" variant="outline">コピー</Button>
                      </Clipboard.Trigger>
                    </HStack>
                  </Clipboard.Root>
                </Box>
              ) : (
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
              )}
            </Dialog.Body>
            <Dialog.Footer>
              {createdToken ? (
                <Button onClick={() => handleClose(false)}>閉じる</Button>
              ) : (
                <HStack>
                  <Button variant="outline" onClick={() => handleClose(false)}>
                    キャンセル
                  </Button>
                  <Button
                    colorPalette="blue"
                    onClick={handleCreate}
                    loading={loading}
                    disabled={!name.trim()}
                  >
                    作成
                  </Button>
                </HStack>
              )}
            </Dialog.Footer>
            {!createdToken && <Dialog.CloseTrigger />}
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  )
}
