import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { notFound } from 'next/navigation';
import { Suspense } from 'react';
import { BoardProjectDetailContent } from '@/components/board-project-detail/board-project-detail-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string }>;
}

export default async function BoardProjectDetailPage({ params }: Props) {
  const { repositoryId, boardProjectId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  const projectOptions = $api.queryOptions('get', '/api/v1/board-projects/{board_project_id}', {
    params: { path: { board_project_id: boardProjectId } },
  });

  const runsOptions = $api.queryOptions(
    'get',
    '/api/v1/board-projects/{board_project_id}/board-runs',
    {
      params: { path: { board_project_id: boardProjectId }, query: { limit: 5 } },
    },
  );

  // Primary resource: await + notFound check
  const projectResult = await queryClient
    .fetchQuery({
      ...projectOptions,
      queryFn: async () => {
        const { data, error } = await serverClient.GET(
          '/api/v1/board-projects/{board_project_id}',
          {
            params: { path: { board_project_id: boardProjectId } },
          },
        );
        if (error) throw error;
        return data;
      },
    })
    .catch(() => null);

  if (!projectResult) {
    notFound();
  }

  // Secondary resource: no await (Streaming SSR)
  queryClient.prefetchQuery({
    ...runsOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/board-projects/{board_project_id}/board-runs',
        {
          params: { path: { board_project_id: boardProjectId }, query: { limit: 5 } },
        },
      );
      if (error) throw new Error('Failed to fetch board runs');
      return data;
    },
  });

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<Box p={8}>Loading...</Box>}>
        <BoardProjectDetailContent repositoryId={repositoryId} boardProjectId={boardProjectId} />
      </Suspense>
    </HydrationBoundary>
  );
}
