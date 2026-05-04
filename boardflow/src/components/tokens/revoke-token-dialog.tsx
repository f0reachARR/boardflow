'use client';

import { Button, Dialog, HStack, Portal, Text } from '@chakra-ui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { $api } from '@/lib/api/react-query';

interface Props {
  repositoryId: string;
  tokenId: string;
  tokenName: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function RevokeTokenDialog({
  repositoryId,
  tokenId,
  tokenName,
  open,
  onOpenChange,
}: Props) {
  const [error, setError] = useState('');
  const queryClient = useQueryClient();

  const { mutate, isPending } = $api.useMutation(
    'post',
    '/api/v1/repositories/{github_repository_id}/api-tokens/{token_id}/revoke',
    {
      onSuccess: () => {
        queryClient.invalidateQueries({
          queryKey: ['get', '/api/v1/repositories/{github_repository_id}/api-tokens'],
        });
        onOpenChange(false);
      },
      onError: (err) => {
        setError(err.error?.message ?? 'トークンの失効に失敗しました');
      },
    },
  );

  const handleRevoke = () => {
    setError('');
    mutate({
      params: { path: { github_repository_id: Number(repositoryId), token_id: tokenId } },
    });
  };

  return (
    <Dialog.Root
      role='alertdialog'
      open={open}
      onOpenChange={(e) => onOpenChange(e.open)}
      closeOnInteractOutside={!isPending}
      closeOnEscape={!isPending}
    >
      <Portal>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>トークンを失効</Dialog.Title>
            </Dialog.Header>
            <Dialog.Body>
              <Text>トークン「{tokenName}」を失効しますか？この操作は取り消せません。</Text>
              {error && (
                <Text color='red.500' mt={3} fontSize='sm'>
                  {error}
                </Text>
              )}
            </Dialog.Body>
            <Dialog.Footer>
              <HStack>
                <Button variant='outline' onClick={() => onOpenChange(false)} disabled={isPending}>
                  キャンセル
                </Button>
                <Button colorPalette='red' onClick={handleRevoke} loading={isPending}>
                  失効する
                </Button>
              </HStack>
            </Dialog.Footer>
            {!isPending && <Dialog.CloseTrigger />}
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
