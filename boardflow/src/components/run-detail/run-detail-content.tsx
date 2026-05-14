'use client';

import { Box, VStack } from '@chakra-ui/react';
import { ArtifactViewerSection } from '@/components/artifact-viewer/artifact-viewer-section';
import { ArtifactSummarySection } from '@/components/run-detail/artifact-summary-section';
import { ArtifactsTable } from '@/components/run-detail/artifacts-table';
import { RunChecksSection } from '@/components/run-detail/run-checks-section';
import { RunDiffSummaryCard } from '@/components/run-detail/run-diff-summary-card';
import { RunHeader } from '@/components/run-detail/run-header';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import type { Artifact, DiffResponse, ViewerEntry } from '@/lib/api/schema-types';
import { shortId } from '@/lib/format';
import { routes } from '@/lib/routes';

interface Props {
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
}

export function RunDetailContent({ repositoryId, boardProjectId, boardRunId }: Props) {
  const { data: run } = $api.useSuspenseQuery('get', '/api/v1/board-runs/{board_run_id}', {
    params: { path: { board_run_id: boardRunId } },
  });

  const { data: artifactsData } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-runs/{board_run_id}/artifacts',
    {
      params: { path: { board_run_id: boardRunId } },
    },
  );

  const { data: viewerData } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-runs/{board_run_id}/viewer-sources',
    {
      params: { path: { board_run_id: boardRunId } },
    },
  );

  const { data: project } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-projects/{board_project_id}',
    {
      params: { path: { board_project_id: boardProjectId } },
    },
  );

  const { data: diffData, error: diffError } = $api.useQuery(
    'get',
    '/api/v1/board-runs/{board_run_id}/diff',
    {
      params: { path: { board_run_id: boardRunId } },
    },
  );

  const artifacts: Artifact[] = artifactsData?.items ?? [];
  const viewers: Record<string, ViewerEntry> = viewerData?.viewers ?? {};
  const diff: DiffResponse | null = diffData ?? null;
  const diffErrorMessage =
    diffError && (diffError as Record<string, unknown>)?.error
      ? ((diffError as Record<string, { message?: string }>).error?.message ??
        'Failed to load diff data.')
      : null;

  return (
    <Box>
      {project && (
        <Breadcrumb
          items={[
            { label: 'Repositories', href: routes.repositories() },
            {
              label: `${project.repository.owner}/${project.repository.name}`,
              href: routes.repository(repositoryId),
            },
            {
              label: project.display_name,
              href: routes.board(repositoryId, boardProjectId),
            },
            { label: 'Runs', href: routes.runs(repositoryId, boardProjectId) },
            { label: shortId(boardRunId) },
          ]}
        />
      )}
      <VStack align='stretch' gap={6}>
        <RunHeader run={run} />
        <RunChecksSection
          checks={run.checks}
          repositoryId={repositoryId}
          boardProjectId={boardProjectId}
          boardRunId={boardRunId}
        />
        <ArtifactSummarySection artifactSummary={run.artifact_summary} />
        <ArtifactsTable artifacts={artifacts} />
        <RunDiffSummaryCard
          diff={diff}
          diffErrorMessage={diffErrorMessage}
          repositoryId={repositoryId}
          boardProjectId={boardProjectId}
          boardRunId={boardRunId}
        />
        <ArtifactViewerSection
          viewers={viewers}
          expiresAt={viewerData?.expires_at}
          boardRunId={boardRunId}
        />
      </VStack>
    </Box>
  );
}
