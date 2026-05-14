'use client';

import { Badge, Box, Heading, HStack, Table, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import { ArtifactViewerSection } from '@/components/artifact-viewer/artifact-viewer-section';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import type { Artifact, DiffResponse, ViewerEntry } from '@/lib/api/schema-types';
import { parseDiffSummary } from '@/lib/domain/diff-summary';
import { artifactStatusColor, boardRunStatusColor, checkStatusColor } from '@/lib/domain/status';
import { formatBytes, formatDateTime, shortId, shortSha } from '@/lib/format';
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
        {/* Header */}
        <Box>
          <HStack gap={3} mb={2}>
            <Heading size='lg'>Run {run.board_run_id}</Heading>
            <Badge colorPalette={boardRunStatusColor(run.status)} size='lg'>
              {run.status}
            </Badge>
          </HStack>
          <HStack gap={4} fontSize='sm' color='gray.600'>
            <Text fontFamily='mono'>{shortSha(run.commit_sha)}</Text>
            <Text>{run.branch}</Text>
            <Text>Created {formatDateTime(run.created_at)}</Text>
            {run.completed_at && <Text>Completed {formatDateTime(run.completed_at)}</Text>}
          </HStack>
        </Box>

        {/* Checks */}
        {run.checks.length > 0 && (
          <Box>
            <Heading size='md' mb={3}>
              Checks
            </Heading>
            <HStack gap={4}>
              {run.checks.map((check) => (
                <Box
                  key={check.kind}
                  borderWidth='1px'
                  borderRadius='md'
                  p={4}
                  bg='white'
                  minW='200px'
                >
                  <HStack justify='space-between' mb={2}>
                    <Text fontWeight='bold' textTransform='uppercase'>
                      {check.kind}
                    </Text>
                    <Badge colorPalette={checkStatusColor(check.status)}>{check.status}</Badge>
                  </HStack>
                  <HStack gap={3} fontSize='sm'>
                    {check.error_count > 0 && (
                      <Text color='red.500'>{check.error_count} errors</Text>
                    )}
                    {check.warning_count > 0 && (
                      <Text color='orange.500'>{check.warning_count} warnings</Text>
                    )}
                    {check.notice_count > 0 && (
                      <Text color='gray.500'>{check.notice_count} notices</Text>
                    )}
                    {check.error_count === 0 &&
                      check.warning_count === 0 &&
                      check.notice_count === 0 && <Text color='green.500'>No issues</Text>}
                  </HStack>
                  {check.error_count + check.warning_count + check.notice_count > 0 && (
                    <Link
                      href={routes.runChecks(repositoryId, boardProjectId, boardRunId, check.kind)}
                    >
                      <Text
                        color='blue.600'
                        fontSize='sm'
                        mt={2}
                        _hover={{ textDecoration: 'underline' }}
                      >
                        View {check.error_count + check.warning_count + check.notice_count} findings
                      </Text>
                    </Link>
                  )}
                </Box>
              ))}
            </HStack>
          </Box>
        )}

        {/* Artifact Summary */}
        <Box>
          <Heading size='md' mb={3}>
            Artifact Summary
          </Heading>
          <HStack gap={4} fontSize='sm'>
            <Badge colorPalette='green'>{run.artifact_summary.available} available</Badge>
            {run.artifact_summary.missing > 0 && (
              <Badge colorPalette='orange'>{run.artifact_summary.missing} missing</Badge>
            )}
            {run.artifact_summary.failed > 0 && (
              <Badge colorPalette='red'>{run.artifact_summary.failed} failed</Badge>
            )}
            {run.artifact_summary.skipped > 0 && (
              <Badge colorPalette='gray'>{run.artifact_summary.skipped} skipped</Badge>
            )}
          </HStack>
        </Box>

        {/* Artifacts Table */}
        {artifacts.length > 0 && (
          <Box>
            <Heading size='md' mb={3}>
              Artifacts
            </Heading>
            <Table.Root size='sm' variant='outline'>
              <Table.Header>
                <Table.Row>
                  <Table.ColumnHeader>Type</Table.ColumnHeader>
                  <Table.ColumnHeader>Status</Table.ColumnHeader>
                  <Table.ColumnHeader>Filename</Table.ColumnHeader>
                  <Table.ColumnHeader>Size</Table.ColumnHeader>
                </Table.Row>
              </Table.Header>
              <Table.Body>
                {artifacts.map((artifact, idx) => (
                  <Table.Row key={artifact.artifact_id ?? idx}>
                    <Table.Cell>
                      <Text fontSize='sm'>{artifact.type}</Text>
                    </Table.Cell>
                    <Table.Cell>
                      <Badge colorPalette={artifactStatusColor(artifact.status)}>
                        {artifact.status}
                      </Badge>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize='sm' fontFamily='mono'>
                        {artifact.filename ?? artifact.status_reason ?? '—'}
                      </Text>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize='sm' color='gray.600'>
                        {artifact.size_bytes ? formatBytes(artifact.size_bytes) : '—'}
                      </Text>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Root>
          </Box>
        )}

        {/* Changes from Baseline */}
        {diffErrorMessage && (
          <Box>
            <Heading size='md' mb={3}>
              Changes from Baseline
            </Heading>
            <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
              <Text fontSize='sm' color='red.500'>
                {diffErrorMessage}
              </Text>
            </Box>
          </Box>
        )}
        {diff && (
          <Box>
            <Heading size='md' mb={3}>
              Changes from Baseline
            </Heading>
            <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
              {diff.status === 'ready' &&
                diff.summary != null &&
                (() => {
                  const summary = parseDiffSummary(diff.summary);
                  return (
                    <VStack align='stretch' gap={2}>
                      {diff.base_board_run_id && (
                        <Text fontSize='sm' color='gray.600'>
                          Compared to run:{' '}
                          <Link
                            href={routes.run(repositoryId, boardProjectId, diff.base_board_run_id)}
                          >
                            <Text
                              as='span'
                              color='blue.600'
                              _hover={{ textDecoration: 'underline' }}
                            >
                              {shortId(diff.base_board_run_id)}
                            </Text>
                          </Link>
                        </Text>
                      )}
                      {summary.fileChanges ? (
                        <HStack gap={4} fontSize='sm'>
                          <Text>
                            Files: +{summary.fileChanges.added} -{summary.fileChanges.removed} ~
                            {summary.fileChanges.changed} ({summary.fileChanges.unchanged}{' '}
                            unchanged)
                          </Text>
                        </HStack>
                      ) : (
                        <Text fontSize='sm' color='gray.500'>
                          File changes: data format not recognized
                        </Text>
                      )}
                      {summary.bomChanges ? (
                        <HStack gap={4} fontSize='sm'>
                          <Text>
                            BOM: +{summary.bomChanges.added} -{summary.bomChanges.removed} ~
                            {summary.bomChanges.changed}
                          </Text>
                        </HStack>
                      ) : (
                        <Text fontSize='sm' color='gray.500'>
                          BOM changes: data format not recognized
                        </Text>
                      )}
                      {summary.checks != null && summary.checks.length > 0 && (
                        <HStack gap={4} fontSize='sm' flexWrap='wrap'>
                          <Text>Checks:</Text>
                          {summary.checks.map(([kind, check]) => (
                            <Text key={kind}>
                              {kind.toUpperCase()} {check.status_change} (
                              {check.error_delta >= 0 ? '+' : ''}
                              {check.error_delta}E, {check.warning_delta >= 0 ? '+' : ''}
                              {check.warning_delta}W)
                            </Text>
                          ))}
                        </HStack>
                      )}
                      {summary.artifactChanges ? (
                        <HStack gap={4} fontSize='sm'>
                          <Text>
                            Artifacts: +{summary.artifactChanges.added} -
                            {summary.artifactChanges.removed} ~{summary.artifactChanges.changed}
                          </Text>
                        </HStack>
                      ) : (
                        <Text fontSize='sm' color='gray.500'>
                          Artifact changes: data format not recognized
                        </Text>
                      )}
                      <Link href={routes.runDiff(repositoryId, boardProjectId, boardRunId)}>
                        <Text
                          color='blue.600'
                          fontSize='sm'
                          mt={2}
                          _hover={{ textDecoration: 'underline' }}
                        >
                          View full diff →
                        </Text>
                      </Link>
                    </VStack>
                  );
                })()}
              {diff.status === 'no_baseline' && (
                <Text fontSize='sm' color='gray.500'>
                  This is the first run. No baseline for comparison.
                </Text>
              )}
              {diff.status === 'failed' && (
                <Text fontSize='sm' color='red.500'>
                  {diff.error_message ?? 'Diff computation failed.'}
                </Text>
              )}
              {diff.status === 'unavailable' && (
                <Text fontSize='sm' color='gray.500'>
                  Diff data is not available.
                </Text>
              )}
            </Box>
          </Box>
        )}

        {/* Viewers */}
        <ArtifactViewerSection
          viewers={viewers}
          expiresAt={viewerData?.expires_at}
          boardRunId={boardRunId}
        />
      </VStack>
    </Box>
  );
}
