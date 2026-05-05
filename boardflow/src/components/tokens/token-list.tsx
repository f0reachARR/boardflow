'use client';

import { Badge, Box, Button, HStack, Table, Text } from '@chakra-ui/react';
import { useState } from 'react';
import { $api } from '@/lib/api/react-query';
import { CreateTokenDialog } from './create-token-dialog';
import { RevokeTokenDialog } from './revoke-token-dialog';

interface Props {
  repositoryId: string;
}

export function TokenList({ repositoryId }: Props) {
  const { data } = $api.useSuspenseQuery(
    'get',
    '/api/v1/repositories/{github_repository_id}/api-tokens',
    {
      params: {
        path: { github_repository_id: Number(repositoryId) },
        query: { limit: 50 },
      },
    },
  );
  const items = data?.items ?? [];

  const [createOpen, setCreateOpen] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<(typeof items)[number] | null>(null);

  return (
    <Box>
      <HStack justify='space-between' mb={4}>
        <Text fontSize='sm' color='gray.600'>
          {items.length} tokens
        </Text>
        <Button colorPalette='blue' size='sm' onClick={() => setCreateOpen(true)}>
          新しいトークンを作成
        </Button>
      </HStack>

      {items.length === 0 ? (
        <Text color='gray.500'>APIトークンはまだありません。</Text>
      ) : (
        <Table.Root size='sm' variant='outline'>
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
                <Table.Cell fontWeight='medium'>{token.name}</Table.Cell>
                <Table.Cell>
                  <Text fontSize='sm' color='gray.600'>
                    {new Date(token.created_at).toLocaleDateString()}
                  </Text>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize='sm' color='gray.600'>
                    {token.last_used_at ? new Date(token.last_used_at).toLocaleDateString() : '—'}
                  </Text>
                </Table.Cell>
                <Table.Cell>
                  {token.revoked_at ? (
                    <Badge colorPalette='red'>Revoked</Badge>
                  ) : (
                    <Badge colorPalette='green'>Active</Badge>
                  )}
                </Table.Cell>
                <Table.Cell>
                  {!token.revoked_at && (
                    <Button
                      size='xs'
                      variant='outline'
                      colorPalette='red'
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
      />

      {revokeTarget && (
        <RevokeTokenDialog
          repositoryId={repositoryId}
          tokenId={revokeTarget.id}
          tokenName={revokeTarget.name}
          open={!!revokeTarget}
          onOpenChange={(open) => {
            if (!open) setRevokeTarget(null);
          }}
        />
      )}
    </Box>
  );
}
