"use client"

import { useState } from "react"
import { Button, Dialog, HStack, Portal, Text } from "@chakra-ui/react"
import { apiClient } from "@/lib/api/client"

interface Props {
  repositoryId: string
  tokenId: string
  tokenName: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onRevoked: () => void
}

export function RevokeTokenDialog({ repositoryId, tokenId, tokenName, open, onOpenChange, onRevoked }: Props) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState("")

  const handleRevoke = async () => {
    setError("")
    setLoading(true)
    const { error: apiError } = await apiClient.POST(
      "/api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke",
      {
        params: { path: { github_repository_id: repositoryId, token_id: tokenId } },
      }
    )
    setLoading(false)
    if (apiError) {
      setError(apiError.error.message)
      return
    }
    onRevoked()
    onOpenChange(false)
  }

  return (
    <Dialog.Root role="alertdialog" open={open} onOpenChange={(e) => onOpenChange(e.open)} closeOnInteractOutside={!loading} closeOnEscape={!loading}>
      <Portal>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>トークンを失効</Dialog.Title>
            </Dialog.Header>
            <Dialog.Body>
              <Text>
                トークン「{tokenName}」を失効しますか？この操作は取り消せません。
              </Text>
              {error && (
                <Text color="red.500" mt={3} fontSize="sm">
                  {error}
                </Text>
              )}
            </Dialog.Body>
            <Dialog.Footer>
              <HStack>
                <Button variant="outline" onClick={() => onOpenChange(false)} disabled={loading}>
                  キャンセル
                </Button>
                <Button colorPalette="red" onClick={handleRevoke} loading={loading}>
                  失効する
                </Button>
              </HStack>
            </Dialog.Footer>
            {!loading && <Dialog.CloseTrigger />}
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  )
}
