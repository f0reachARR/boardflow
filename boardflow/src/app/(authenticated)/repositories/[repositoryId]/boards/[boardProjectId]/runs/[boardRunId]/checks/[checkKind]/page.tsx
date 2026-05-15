import { Box, Heading, Text } from '@chakra-ui/react';
import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { FindingsContent } from '@/components/checks/findings-content';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { prefetchSecondary, withServerFetcher } from '@/lib/api/server-prefetch';
import { getQueryClient } from '@/lib/query-client';

interface Props {
  params: Promise<{
    repositoryId: string;
    boardProjectId: string;
    boardRunId: string;
    checkKind: string;
  }>;
  searchParams: Promise<{ severity?: string }>;
}

const VALID_CHECK_KINDS = ['erc', 'drc'];
const VALID_SEVERITIES = ['error', 'warning', 'notice'];

export default async function FindingsPage({ params, searchParams }: Props) {
  const { repositoryId, boardProjectId, boardRunId, checkKind } = await params;
  const { severity } = await searchParams;

  if (!VALID_CHECK_KINDS.includes(checkKind)) {
    return (
      <Box>
        <Heading size='lg' mb={4}>
          Invalid Check Kind
        </Heading>
        <Text color='red.500'>
          &quot;{checkKind}&quot; is not a valid check kind. Supported values:{' '}
          {VALID_CHECK_KINDS.join(', ')}.
        </Text>
      </Box>
    );
  }

  if (severity && !VALID_SEVERITIES.includes(severity)) {
    return (
      <Box>
        <Heading size='lg' mb={4}>
          {checkKind.toUpperCase()} Findings
        </Heading>
        <Text color='red.500'>
          &quot;{severity}&quot; is not a valid severity filter. Supported values:{' '}
          {VALID_SEVERITIES.join(', ')}.
        </Text>
      </Box>
    );
  }

  const validCheckKind = checkKind as 'erc' | 'drc';
  const validSeverityParam = severity as 'error' | 'warning' | 'notice' | undefined;

  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions(
        'get',
        '/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings',
        {
          params: {
            path: { board_run_id: boardRunId, check_kind: validCheckKind },
            query: validSeverityParam ? { severity: validSeverityParam } : undefined,
          },
        },
      ),
      () =>
        serverClient.GET('/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings', {
          params: {
            path: { board_run_id: boardRunId, check_kind: validCheckKind },
            query: validSeverityParam ? { severity: validSeverityParam } : undefined,
          },
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
        <FindingsContent
          repositoryId={repositoryId}
          boardProjectId={boardProjectId}
          boardRunId={boardRunId}
          checkKind={validCheckKind}
          severity={validSeverityParam}
        />
      </Suspense>
    </HydrationBoundary>
  );
}
