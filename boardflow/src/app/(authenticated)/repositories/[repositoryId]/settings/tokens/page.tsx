import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { TokensPageContent } from '@/components/tokens/tokens-page-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string }>;
}

export default async function TokensPage({ params }: Props) {
  const { repositoryId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  const repoOptions = $api.queryOptions('get', '/api/v1/repositories/{github_repository_id}', {
    params: { path: { github_repository_id: Number(repositoryId) } },
  });

  const tokensOptions = $api.queryOptions(
    'get',
    '/api/v1/repositories/{github_repository_id}/api-tokens',
    {
      params: {
        path: { github_repository_id: Number(repositoryId) },
        query: { limit: 50 },
      },
    },
  );

  queryClient.prefetchQuery({
    ...repoOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/repositories/{github_repository_id}',
        {
          params: { path: { github_repository_id: Number(repositoryId) } },
        },
      );
      if (error) throw new Error('Failed to fetch repository');
      return data;
    },
  });

  queryClient.prefetchQuery({
    ...tokensOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/repositories/{github_repository_id}/api-tokens',
        {
          params: {
            path: { github_repository_id: Number(repositoryId) },
            query: { limit: 50 },
          },
        },
      );
      if (error) throw new Error('Failed to fetch tokens');
      return data;
    },
  });

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box>Loading...</Box>}>
        <TokensPageContent repositoryId={repositoryId} />
      </Suspense>
    </HydrationBoundary>
  );
}
