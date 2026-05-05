import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { DiffContent } from '@/components/diff/diff-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>;
}

export default async function DiffPage({ params }: Props) {
  const { repositoryId, boardProjectId, boardRunId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  const diffOptions = $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}/diff', {
    params: { path: { board_run_id: boardRunId } },
  });

  const projectOptions = $api.queryOptions('get', '/api/v1/board-projects/{board_project_id}', {
    params: { path: { board_project_id: boardProjectId } },
  });

  queryClient.prefetchQuery({
    ...diffOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET('/api/v1/board-runs/{board_run_id}/diff', {
        params: { path: { board_run_id: boardRunId } },
      });
      if (error) throw new Error('Failed to fetch diff');
      return data;
    },
  });

  queryClient.prefetchQuery({
    ...projectOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET('/api/v1/board-projects/{board_project_id}', {
        params: { path: { board_project_id: boardProjectId } },
      });
      if (error) throw new Error('Failed to fetch project');
      return data;
    },
  });

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
