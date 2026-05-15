import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { BoardProjectDetailContent } from '@/components/board-project-detail/board-project-detail-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { fetchPrimary, prefetchSecondary, withServerFetcher } from '@/lib/api/server-prefetch';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string }>;
}

export default async function BoardProjectDetailPage({ params }: Props) {
  const { repositoryId, boardProjectId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  // Primary resource: await + notFound check
  await fetchPrimary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/board-projects/{board_project_id}', {
        params: { path: { board_project_id: boardProjectId } },
      }),
      () =>
        serverClient.GET('/api/v1/board-projects/{board_project_id}', {
          params: { path: { board_project_id: boardProjectId } },
        }),
    ),
  );

  // Secondary resource: no await (Streaming SSR)
  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/board-projects/{board_project_id}/board-runs', {
        params: { path: { board_project_id: boardProjectId }, query: { limit: 5 } },
      }),
      () =>
        serverClient.GET('/api/v1/board-projects/{board_project_id}/board-runs', {
          params: { path: { board_project_id: boardProjectId }, query: { limit: 5 } },
        }),
    ),
  );

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box p={8}>Loading...</Box>}>
        <BoardProjectDetailContent repositoryId={repositoryId} boardProjectId={boardProjectId} />
      </Suspense>
    </HydrationBoundary>
  );
}
