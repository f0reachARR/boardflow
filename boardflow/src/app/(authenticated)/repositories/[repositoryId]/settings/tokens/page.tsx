import { Box, Heading, VStack } from '@chakra-ui/react';
import { notFound } from 'next/navigation';
import { TokenList } from '@/components/tokens/token-list';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { createServerClient } from '@/lib/api/server';

interface Props {
  params: Promise<{ repositoryId: string }>;
}

export default async function TokensPage({ params }: Props) {
  const { repositoryId } = await params;
  const client = await createServerClient();

  const [repoRes, tokensRes] = await Promise.all([
    client.GET('/api/v1/repositories/{github_repository_id}', {
      params: { path: { github_repository_id: Number(repositoryId) } },
    }),
    client.GET('/api/v1/repositories/{github_repository_id}/api-tokens', {
      params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
    }),
  ]);

  if (repoRes.error) {
    notFound();
  }

  const repo = repoRes.data;
  const fetchError = tokensRes.error ? 'トークン一覧の取得に失敗しました' : undefined;
  const tokens = tokensRes.data?.items ?? [];
  const hasMore = tokensRes.data?.has_more ?? false;
  const nextCursor = tokensRes.data?.next_cursor ?? null;

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
        <TokenList
          items={tokens}
          repositoryId={repositoryId}
          hasMore={hasMore}
          nextCursor={nextCursor}
          fetchError={fetchError}
        />
      </VStack>
    </Box>
  );
}
