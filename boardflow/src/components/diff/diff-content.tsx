'use client';

import { Box, VStack } from '@chakra-ui/react';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import { parseDiffSummary } from '@/lib/domain/diff-summary';
import { shortId } from '@/lib/format';
import { routes } from '@/lib/routes';
import { ArtifactChangesSection } from './artifact-changes-section';
import { BomChangesSection } from './bom-changes-section';
import { ChecksSection } from './checks-section';
import { DiffHeader } from './diff-header';
import { DiffStatusMessage } from './diff-status-message';
import { FileChangesSection } from './file-changes-section';
import { PreviewLinksSection } from './preview-links-section';

interface Props {
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
}

export function DiffContent({ repositoryId, boardProjectId, boardRunId }: Props) {
  const { data: diff } = $api.useSuspenseQuery('get', '/api/v1/board-runs/{board_run_id}/diff', {
    params: { path: { board_run_id: boardRunId } },
  });

  const { data: project } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-projects/{board_project_id}',
    {
      params: { path: { board_project_id: boardProjectId } },
    },
  );

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
            {
              label: shortId(boardRunId),
              href: routes.run(repositoryId, boardProjectId, boardRunId),
            },
            { label: 'Diff' },
          ]}
        />
      )}

      <VStack align='stretch' gap={6}>
        <DiffHeader
          status={diff.status}
          baseBoardRunId={diff.base_board_run_id ?? null}
          boardRunId={boardRunId}
          repositoryId={repositoryId}
          boardProjectId={boardProjectId}
          createdAt={diff.created_at}
        />

        <DiffStatusMessage status={diff.status} errorMessage={diff.error_message ?? null} />

        {diff.status === 'ready' &&
          diff.summary != null &&
          (() => {
            const summary = parseDiffSummary(diff.summary);
            return (
              <>
                <FileChangesSection summary={summary} metadata={diff.metadata ?? null} />
                <BomChangesSection summary={summary} metadata={diff.metadata ?? null} />
                <ChecksSection summary={summary} />
                <ArtifactChangesSection summary={summary} metadata={diff.metadata ?? null} />
                <PreviewLinksSection
                  metadata={diff.metadata ?? null}
                  repositoryId={repositoryId}
                  boardProjectId={boardProjectId}
                  boardRunId={boardRunId}
                  baseRunId={diff.base_board_run_id ?? null}
                />
              </>
            );
          })()}
      </VStack>
    </Box>
  );
}
