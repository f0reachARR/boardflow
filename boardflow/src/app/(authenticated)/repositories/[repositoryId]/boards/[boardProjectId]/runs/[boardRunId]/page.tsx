import { Box } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { RunDetailContent } from '@/components/run-detail/run-detail-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>;
}

export default async function RunDetailPage({ params }: Props) {
  const { repositoryId, boardProjectId, boardRunId } = await params;
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  const runOptions = $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}', {
    params: { path: { board_run_id: boardRunId } },
  });

  const artifactsOptions = $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}/artifacts', {
    params: { path: { board_run_id: boardRunId } },
  });

  const viewerOptions = $api.queryOptions(
    'get',
    '/api/v1/board-runs/{board_run_id}/viewer-sources',
    {
      params: { path: { board_run_id: boardRunId } },
    },
  );

  const projectOptions = $api.queryOptions('get', '/api/v1/board-projects/{board_project_id}', {
    params: { path: { board_project_id: boardProjectId } },
  });

  const diffOptions = $api.queryOptions('get', '/api/v1/board-runs/{board_run_id}/diff', {
    params: { path: { board_run_id: boardRunId } },
  });

  queryClient.prefetchQuery({
    ...runOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET('/api/v1/board-runs/{board_run_id}', {
        params: { path: { board_run_id: boardRunId } },
      });
      if (error) throw new Error('Failed to fetch run');
      return data;
    },
  });

  queryClient.prefetchQuery({
    ...artifactsOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/board-runs/{board_run_id}/artifacts',
        {
          params: { path: { board_run_id: boardRunId } },
        },
      );
      if (error) throw new Error('Failed to fetch artifacts');
      return data;
    },
  });

  queryClient.prefetchQuery({
    ...viewerOptions,
    queryFn: async () => {
      const { data, error } = await serverClient.GET(
        '/api/v1/board-runs/{board_run_id}/viewer-sources',
        {
          params: { path: { board_run_id: boardRunId } },
        },
      );
      if (error) throw new Error('Failed to fetch viewer sources');
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
