import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { RepositoryDetailContent } from '@/components/repository-detail/repository-detail-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { fetchPrimary, prefetchSecondary, withServerFetcher } from '@/lib/api/server-prefetch';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string }>;
}

export default async function RepositoryDetailPage({ params }: Props) {
  const { repositoryId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  // Primary resource: await + notFound check
  await fetchPrimary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/repositories/{github_repository_id}', {
        params: { path: { github_repository_id: Number(repositoryId) } },
      }),
      () =>
        serverClient.GET('/api/v1/repositories/{github_repository_id}', {
          params: { path: { github_repository_id: Number(repositoryId) } },
        }),
    ),
  );

  // Secondary resource: no await (Streaming SSR)
  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/repositories/{github_repository_id}/board-projects', {
        params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
      }),
      () =>
        serverClient.GET('/api/v1/repositories/{github_repository_id}/board-projects', {
          params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
        }),
      'Failed to fetch board projects',
    ),
  );

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box p={8}>Loading...</Box>}>
        <RepositoryDetailContent repositoryId={repositoryId} />
      </Suspense>
    </HydrationBoundary>
  );
}
