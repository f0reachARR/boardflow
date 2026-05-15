import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { DiffContent } from '@/components/diff/diff-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { prefetchSecondary, withServerFetcher } from '@/lib/api/server-prefetch';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>;
}

export default async function DiffPage({ params }: Props) {
  const { repositoryId, boardProjectId, boardRunId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}/diff', {
        params: { path: { board_run_id: boardRunId } },
      }),
      () =>
        serverClient.GET('/api/v1/board-runs/{board_run_id}/diff', {
          params: { path: { board_run_id: boardRunId } },
        }),
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
    ),
  );

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box p={8}>Loading...</Box>}>
        <DiffContent
          repositoryId={repositoryId}
          boardProjectId={boardProjectId}
          boardRunId={boardRunId}
        />
      </Suspense>
    </HydrationBoundary>
  );
}
