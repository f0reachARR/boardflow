import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { RunDetailContent } from '@/components/run-detail/run-detail-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { fetchPrimary, prefetchSecondary, withServerFetcher } from '@/lib/api/server-prefetch';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>;
}

export default async function RunDetailPage({ params }: Props) {
  const { repositoryId, boardProjectId, boardRunId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  // Primary resource: await + notFound check
  await fetchPrimary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}', {
        params: { path: { board_run_id: boardRunId } },
      }),
      () =>
        serverClient.GET('/api/v1/board-runs/{board_run_id}', {
          params: { path: { board_run_id: boardRunId } },
        }),
    ),
  );

  // Secondary resources: no await (Streaming SSR)
  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}/artifacts', {
        params: { path: { board_run_id: boardRunId } },
      }),
      () =>
        serverClient.GET('/api/v1/board-runs/{board_run_id}/artifacts', {
          params: { path: { board_run_id: boardRunId } },
        }),
      'Failed to fetch artifacts',
    ),
  );

  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}/viewer-sources', {
        params: { path: { board_run_id: boardRunId } },
      }),
      () =>
        serverClient.GET('/api/v1/board-runs/{board_run_id}/viewer-sources', {
          params: { path: { board_run_id: boardRunId } },
        }),
      'Failed to fetch viewer sources',
    ),
  );

  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/board-projects/{board_project_id}', {
        params: { path: { board_project_id: boardProjectId } },
      }),
      () =>
        serverClient.GET('/api/v1/board-projects/{board_project_id}', {
          params: { path: { board_project_id: boardProjectId } },
        }),
      'Failed to fetch board project',
    ),
  );

  // Note: diff is NOT prefetched here. The client component fetches it
  // via useQuery (non-Suspense) and handles 404 as a normal case.

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box p={8}>Loading...</Box>}>
        <RunDetailContent
          repositoryId={repositoryId}
          boardProjectId={boardProjectId}
          boardRunId={boardRunId}
        />
      </Suspense>
    </HydrationBoundary>
  );
}
