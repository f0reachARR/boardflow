import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { RunsListContent } from '@/components/runs/runs-list-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string }>;
}

export default async function RunsPage({ params }: Props) {
  const { repositoryId, boardProjectId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  const projectOptions = $api.queryOptions(
    'get',
    '/api/v1/board-projects/{board_project_id}',
    {
      params: { path: { board_project_id: boardProjectId } },
    },
  );

  const runsOptions = $api.queryOptions(
    'get',
    '/api/v1/board-projects/{board_project_id}/board-runs',
    {
      params: { path: { board_project_id: boardProjectId }, query: { limit: 50 } },
    },
  );

  queryClient.prefetchQuery({
    ...projectOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/board-projects/{board_project_id}',
        {
          params: { path: { board_project_id: boardProjectId } },
        },
      );
      if (error) throw new Error('Failed to fetch board project');
      return data;
    },
  });

  queryClient.prefetchQuery({
    ...runsOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/board-projects/{board_project_id}/board-runs',
        {
          params: { path: { board_project_id: boardProjectId }, query: { limit: 50 } },
        },
      );
      if (error) throw new Error('Failed to fetch board runs');
      return data;
    },
  });

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box p={8}>Loading...</Box>}>
        <RunsListContent repositoryId={repositoryId} boardProjectId={boardProjectId} />
      </Suspense>
    </HydrationBoundary>
  );
}
