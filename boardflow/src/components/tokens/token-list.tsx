"use client"

import { useState } from "react"
import { Badge, Box, Button, HStack, Table, Text } from "@chakra-ui/react"
import { useRouter } from "next/navigation"
import type { ApiToken } from "@/lib/api/schema"
import { CreateTokenDialog } from "./create-token-dialog"
import { RevokeTokenDialog } from "./revoke-token-dialog"

interface Props {
  items: ApiToken[]
  repositoryId: string
  hasMore: boolean
  nextCursor: string | null
}

export function TokenList({ items, repositoryId }: Props) {
  const router = useRouter()
  const [createOpen, setCreateOpen] = useState(false)
  const [revokeTarget, setRevokeTarget] = useState<ApiToken | null>(null)

  const handleCreated = () => {
    router.refresh()
  }

  const handleRevoked = () => {
    router.refresh()
  }

  return (
    <Box>
      <HStack justify="space-between" mb={4}>
        <Text fontSize="sm" color="gray.600">{items.length} tokens</Text>
        <Button colorPalette="blue" size="sm" onClick={() => setCreateOpen(true)}>
          新しいトークンを作成
        </Button>
      </HStack>

      {items.length === 0 ? (
        <Text color="gray.500">APIトークンはまだありません。</Text>
      ) : (
        <Table.Root size="sm" variant="outline">
          <Table.Header>
            <Table.Row>
              <Table.ColumnHeader>Name</Table.ColumnHeader>
              <Table.ColumnHeader>Created</Table.ColumnHeader>
              <Table.ColumnHeader>Last Used</Table.ColumnHeader>
              <Table.ColumnHeader>Status</Table.ColumnHeader>
              <Table.ColumnHeader>Actions</Table.ColumnHeader>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {items.map((token) => (
              <Table.Row key={token.id}>
                <Table.Cell fontWeight="medium">{token.name}</Table.Cell>
                <Table.Cell>
                  <Text fontSize="sm" color="gray.600">
                    {new Date(token.created_at).toLocaleDateString()}
                  </Text>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize="sm" color="gray.600">
                    {token.last_used_at
                      ? new Date(token.last_used_at).toLocaleDateString()
                      : "—"}
                  </Text>
                </Table.Cell>
                <Table.Cell>
                  {token.revoked_at ? (
                    <Badge colorPalette="red">Revoked</Badge>
                  ) : (
                    <Badge colorPalette="green">Active</Badge>
                  )}
                </Table.Cell>
                <Table.Cell>
                  {!token.revoked_at && (
                    <Button
                      size="xs"
                      variant="outline"
                      colorPalette="red"
                      onClick={() => setRevokeTarget(token)}
                    >
                      Revoke
                    </Button>
                  )}
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Root>
      )}

      <CreateTokenDialog
        repositoryId={repositoryId}
        open={createOpen}
        onOpenChange={setCreateOpen}
        onCreated={handleCreated}
      />

      {revokeTarget && (
        <RevokeTokenDialog
          repositoryId={repositoryId}
          tokenId={revokeTarget.id}
          tokenName={revokeTarget.name}
          open={!!revokeTarget}
          onOpenChange={(open) => { if (!open) setRevokeTarget(null) }}
          onRevoked={handleRevoked}
        />
      )}
    </Box>
  )
}
