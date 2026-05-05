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
import { useForm } from '@tanstack/react-form';
import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { z } from 'zod';
import { $api } from '@/lib/api/react-query';

const createTokenSchema = z.object({
  name: z
    .string()
    .min(1, '名前は1文字以上で入力してください')
    .max(100, '名前は100文字以内で入力してください'),
});

interface Props {
  repositoryId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function CreateTokenDialog({ repositoryId, open, onOpenChange }: Props) {
  const [serverError, setServerError] = useState('');
  const [createdToken, setCreatedToken] = useState<string | null>(null);
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

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(e) => handleClose(e.open)}
      closeOnInteractOutside={!createdToken && !form.state.isSubmitting}
      closeOnEscape={!createdToken && !form.state.isSubmitting}
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
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    form.handleSubmit();
                  }}
                >
                  <form.Field name='name'>
                    {(field) => (
                      <Field.Root
                        invalid={
                          (field.state.meta.isTouched && field.state.meta.errors.length > 0) ||
                          !!serverError
                        }
                      >
                        <Field.Label>トークン名</Field.Label>
                        <Input
                          placeholder='例: CI用トークン'
                          value={field.state.value}
                          onChange={(e) => field.handleChange(e.target.value)}
                          onBlur={field.handleBlur}
                          maxLength={100}
                        />
                        {field.state.meta.isTouched && field.state.meta.errors.length > 0 && (
                          <Field.ErrorText>
                            {field.state.meta.errors
                              .map((e) =>
                                typeof e === 'string'
                                  ? e
                                  : ((e as { message?: string })?.message ?? String(e)),
                              )
                              .join(', ')}
                          </Field.ErrorText>
                        )}
                        {serverError && <Field.ErrorText>{serverError}</Field.ErrorText>}
                      </Field.Root>
                    )}
                  </form.Field>
                  <Dialog.Footer>
                    <form.Subscribe
                      selector={(state) => [state.canSubmit, state.isSubmitting] as const}
                    >
                      {([canSubmit, isSubmitting]) => (
                        <HStack>
                          <Button
                            variant='outline'
                            onClick={() => handleClose(false)}
                            disabled={isSubmitting}
                          >
                            キャンセル
                          </Button>
                          <Button
                            type='submit'
                            colorPalette='blue'
                            loading={isSubmitting}
                            disabled={!canSubmit}
                          >
                            作成
                          </Button>
                        </HStack>
                      )}
                    </form.Subscribe>
                  </Dialog.Footer>
                </form>
              )}
            </Dialog.Body>
            {createdToken && (
              <Dialog.Footer>
                <Button onClick={() => handleClose(false)}>閉じる</Button>
              </Dialog.Footer>
            )}
            {!createdToken && !form.state.isSubmitting && <Dialog.CloseTrigger />}
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
  );
}
