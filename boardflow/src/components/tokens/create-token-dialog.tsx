'use client';

import {
  Box,
  Button,
  Clipboard,
  Dialog,
  Field,
  HStack,
  Input,
  Portal,
  Text,
} from '@chakra-ui/react';
import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { $api } from '@/lib/api/react-query';

interface Props {
  repositoryId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CreateTokenDialog({ repositoryId, open, onOpenChange }: Props) {
  const [name, setName] = useState('');
  const [error, setError] = useState('');
  const [createdToken, setCreatedToken] = useState<string | null>(null);
  const queryClient = useQueryClient();

  const { mutate, isPending } = $api.useMutation(
    'post',
    '/api/v1/repositories/{github_repository_id}/api-tokens',
    {
      onSuccess: (data) => {
        if (!data?.token) {
          setError('トークンの作成に失敗しました');
          return;
        }
        setCreatedToken(data.token);
        queryClient.invalidateQueries({
          queryKey: ['get', '/api/v1/repositories/{github_repository_id}/api-tokens'],
        });
      },
      onError: (err) => {
        setError(err.error?.message ?? 'トークンの作成に失敗しました');
      },
    },
  );

  const handleCreate = () => {
    const trimmed = name.trim();
    if (trimmed.length < 1 || trimmed.length > 100) {
      setError('名前は1〜100文字で入力してください');
      return;
    }
    setError('');
    mutate({
      params: { path: { github_repository_id: Number(repositoryId) } },
      body: { name: trimmed },
    });
  };

  const handleClose = (open: boolean) => {
    if (!open) {
      setName('');
      setError('');
      setCreatedToken(null);
    }
    onOpenChange(open);
  };

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(e) => handleClose(e.open)}
      closeOnInteractOutside={!createdToken && !isPending}
      closeOnEscape={!createdToken && !isPending}
    >
      <Portal>
        <Dialog.Backdrop />
        <Dialog.Positioner>
          <Dialog.Content>
            <Dialog.Header>
              <Dialog.Title>
                {createdToken ? 'トークンが作成されました' : '新しいAPIトークンを作成'}
              </Dialog.Title>
            </Dialog.Header>
            <Dialog.Body>
              {createdToken ? (
                <Box>
                  <Text fontWeight='bold' color='orange.600' mb={3}>
                    この画面を閉じるとトークンは二度と表示されません。
                  </Text>
                  <Clipboard.Root value={createdToken}>
                    <Clipboard.Label>トークン</Clipboard.Label>
                    <HStack>
                      <Clipboard.Input />
                      <Clipboard.Trigger asChild>
                        <Button size='sm' variant='outline'>
                          コピー
                        </Button>
                      </Clipboard.Trigger>
                    </HStack>
                  </Clipboard.Root>
                </Box>
              ) : (
                <Field.Root invalid={!!error}>
                  <Field.Label>トークン名</Field.Label>
                  <Input
                    placeholder='例: CI用トークン'
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
                  <Button variant='outline' onClick={() => handleClose(false)} disabled={isPending}>
                    キャンセル
                  </Button>
                  <Button
                    colorPalette='blue'
                    onClick={handleCreate}
                    loading={isPending}
                    disabled={!name.trim()}
                  >
                    作成
                  </Button>
                </HStack>
              )}
            </Dialog.Footer>
            {!createdToken && !isPending && <Dialog.CloseTrigger />}
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
