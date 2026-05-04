import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { notFound } from 'next/navigation';
import { Suspense } from 'react';
import { RepositoryDetailContent } from '@/components/repository-detail/repository-detail-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string }>;
}

export default async function RepositoryDetailPage({ params }: Props) {
  const { repositoryId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  const repoOptions = $api.queryOptions('get', '/api/v1/repositories/{github_repository_id}', {
    params: { path: { github_repository_id: Number(repositoryId) } },
  });

  const projectsOptions = $api.queryOptions(
    'get',
    '/api/v1/repositories/{github_repository_id}/board-projects',
    {
      params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
    },
  );

  // Primary resource: await + notFound check
  const repoResult = await queryClient
    .fetchQuery({
      ...repoOptions,
      queryFn: async () => {
        const { data, error } = await serverClient.GET(
          '/api/v1/repositories/{github_repository_id}',
          {
            params: { path: { github_repository_id: Number(repositoryId) } },
          },
        );
        if (error) throw error;
        return data;
      },
    })
    .catch(() => null);

  if (!repoResult) {
    notFound();
  }

  // Secondary resource: no await (Streaming SSR)
  queryClient.prefetchQuery({
    ...projectsOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/repositories/{github_repository_id}/board-projects',
        {
          params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
        },
      );
      if (error) throw new Error('Failed to fetch board projects');
      return data;
    },
  });

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box p={8}>Loading...</Box>}>
        <RepositoryDetailContent repositoryId={repositoryId} />
      </Suspense>
    </HydrationBoundary>
  );
}
