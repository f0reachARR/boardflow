'use client';

import { Box, Heading, VStack } from '@chakra-ui/react';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import { TokenList } from './token-list';

interface Props {
  repositoryId: string;
}

export function TokensPageContent({ repositoryId }: Props) {
  const { data: repo } = $api.useSuspenseQuery(
    'get',
    '/api/v1/repositories/{github_repository_id}',
    {
      params: { path: { github_repository_id: Number(repositoryId) } },
    },
  );

  return (
    <Box>
      <Breadcrumb
        items={[
          { label: 'Repositories', href: '/repositories' },
          { label: `${repo.owner}/${repo.name}`, href: `/repositories/${repositoryId}` },
          { label: 'API Tokens' },
        ]}
      />
      <VStack align='stretch' gap={6}>
        <Heading size='lg'>API Tokens</Heading>
        <TokenList repositoryId={repositoryId} />
      </VStack>
    </Box>
  );
}
