'use client';

import { Badge, Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import { type ParsedDiffSummary, parseDiffSummary } from '@/lib/domain/diff-summary';
import { isRecord } from '@/lib/domain/guards';
import { diffStatusColor } from '@/lib/domain/status';
import { formatDateTime, shortId } from '@/lib/format';
import { routes } from '@/lib/routes';

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
        {/* Header */}
        <Box>
          <HStack gap={3} mb={2}>
            <Heading size='lg'>Diff</Heading>
            <Badge colorPalette={diffStatusColor(diff.status)} size='lg'>
              {diff.status}
            </Badge>
          </HStack>
          <HStack gap={4} fontSize='sm' color='gray.600'>
            {diff.base_board_run_id && (
              <Text>
                Compared to:{' '}
                <Link href={routes.run(repositoryId, boardProjectId, diff.base_board_run_id)}>
                  <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                    {shortId(diff.base_board_run_id)}
                  </Text>
                </Link>
              </Text>
            )}
            <Text>
              Current:{' '}
              <Link href={routes.run(repositoryId, boardProjectId, boardRunId)}>
                <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                  {shortId(boardRunId)}
                </Text>
              </Link>
            </Text>
            <Text>Created {formatDateTime(diff.created_at)}</Text>
          </HStack>
        </Box>

        {/* Status-based content */}
        {diff.status === 'no_baseline' && (
          <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
            <Text fontSize='sm' color='gray.500'>
              This is the first completed run. No baseline available for comparison.
            </Text>
          </Box>
        )}

        {diff.status === 'unavailable' && (
          <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
            <Text fontSize='sm' color='gray.500'>
              Diff data is not available. The baseline or current run may be missing required data.
            </Text>
          </Box>
        )}

        {diff.status === 'failed' && (
          <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
            <Text fontSize='sm' color='red.500'>
              {diff.error_message ?? 'Diff computation failed.'}
            </Text>
          </Box>
        )}

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

function FileChangesSection({
  summary,
  metadata,
}: {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}) {
  if (!summary.fileChanges) {
    return (
      <Box>
        <Heading size='md' mb={3}>
          File Changes
        </Heading>
        <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
          <Text fontSize='sm' color='gray.500'>
            Data format not recognized
          </Text>
        </Box>
      </Box>
    );
  }

  const { added, removed, changed, unchanged } = summary.fileChanges;

  const fileHashesRaw = metadata?.file_hashes;
  const fileHashes = isRecord(fileHashesRaw) ? fileHashesRaw : null;
  const fileCount = fileHashes ? Object.keys(fileHashes).length : null;

  const changedFiles: string[] = [];
  if (fileHashes) {
    for (const [path, value] of Object.entries(fileHashes)) {
      if (isRecord(value) && typeof value.status === 'string' && value.status !== 'unchanged') {
        changedFiles.push(path);
      }
    }
  }

  return (
    <Box>
      <Heading size='md' mb={3}>
        File Changes
      </Heading>
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <HStack gap={4} fontSize='sm' mb={changedFiles.length > 0 || fileCount !== null ? 3 : 0}>
          <Badge colorPalette='green'>+{added} added</Badge>
          <Badge colorPalette='red'>-{removed} removed</Badge>
          <Badge colorPalette='yellow'>~{changed} changed</Badge>
          <Badge colorPalette='gray'>{unchanged} unchanged</Badge>
        </HStack>
        {fileCount !== null && changedFiles.length === 0 && (
          <Text fontSize='sm' color='gray.500'>
            {fileCount} file(s) tracked in metadata.
          </Text>
        )}
        {changedFiles.length > 0 && (
          <Box mt={2}>
            <Text fontSize='sm' fontWeight='bold' mb={1}>
              Changed files:
            </Text>
            <VStack align='stretch' gap={1}>
              {changedFiles.map((file) => (
                <Text key={file} fontSize='sm' fontFamily='mono' color='gray.700'>
                  {file}
                </Text>
              ))}
            </VStack>
          </Box>
        )}
      </Box>
    </Box>
  );
}

function BomChangesSection({
  summary,
  metadata,
}: {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}) {
  if (!summary.bomChanges) {
    return (
      <Box>
        <Heading size='md' mb={3}>
          BOM Changes
        </Heading>
        <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
          <Text fontSize='sm' color='gray.500'>
            Data format not recognized
          </Text>
        </Box>
      </Box>
    );
  }

  const { added, removed, changed } = summary.bomChanges;

  const bomRaw = metadata?.bom_summary;
  const hasBomData = bomRaw != null;

  return (
    <Box>
      <Heading size='md' mb={3}>
        BOM Changes
      </Heading>
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <HStack gap={4} fontSize='sm' mb={hasBomData ? 3 : 0}>
          <Badge colorPalette='green'>+{added} added</Badge>
          <Badge colorPalette='red'>-{removed} removed</Badge>
          <Badge colorPalette='yellow'>~{changed} changed</Badge>
        </HStack>
        {hasBomData && (
          <Text fontSize='sm' color='gray.500'>
            Detailed BOM data available in metadata.
          </Text>
        )}
      </Box>
    </Box>
  );
}

function ChecksSection({ summary }: { summary: ParsedDiffSummary }) {
  if (summary.checks.length === 0) return null;

  return (
    <Box>
      <Heading size='md' mb={3}>
        ERC/DRC Checks
      </Heading>
      <HStack gap={4} flexWrap='wrap'>
        {summary.checks.map(([kind, check]) => (
          <Box key={kind} borderWidth='1px' borderRadius='md' p={4} bg='white' minW='200px'>
            <Text fontWeight='bold' textTransform='uppercase' mb={2}>
              {kind}
            </Text>
            <VStack align='stretch' gap={1} fontSize='sm'>
              <Text>Status: {check.status_change}</Text>
              <HStack gap={3}>
                <Text
                  color={
                    check.error_delta > 0
                      ? 'red.500'
                      : check.error_delta < 0
                        ? 'green.500'
                        : 'gray.500'
                  }
                >
                  {check.error_delta >= 0 ? '+' : ''}
                  {check.error_delta} errors
                </Text>
                <Text
                  color={
                    check.warning_delta > 0
                      ? 'orange.500'
                      : check.warning_delta < 0
                        ? 'green.500'
                        : 'gray.500'
                  }
                >
                  {check.warning_delta >= 0 ? '+' : ''}
                  {check.warning_delta} warnings
                </Text>
              </HStack>
            </VStack>
          </Box>
        ))}
      </HStack>
    </Box>
  );
}

function ArtifactChangesSection({
  summary,
  metadata,
}: {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}) {
  if (!summary.artifactChanges) {
    return (
      <Box>
        <Heading size='md' mb={3}>
          Artifact Changes
        </Heading>
        <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
          <Text fontSize='sm' color='gray.500'>
            Data format not recognized
          </Text>
        </Box>
      </Box>
    );
  }

  const { added, removed, changed } = summary.artifactChanges;

  const artifactsRaw = metadata?.artifacts_summary;
  const artifactsSummary = isRecord(artifactsRaw) ? artifactsRaw : null;
  const artifactEntries = artifactsSummary ? Object.entries(artifactsSummary) : [];

  return (
    <Box>
      <Heading size='md' mb={3}>
        Artifact Changes
      </Heading>
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <HStack gap={4} fontSize='sm'>
          <Badge colorPalette='green'>+{added} added</Badge>
          <Badge colorPalette='red'>-{removed} removed</Badge>
          <Badge colorPalette='yellow'>~{changed} changed</Badge>
        </HStack>
        {artifactEntries.length > 0 && (
          <VStack align='stretch' gap={1} mt={3}>
            <Text fontSize='sm' fontWeight='bold'>
              Artifact Status Detail:
            </Text>
            {artifactEntries.map(([name, value]) => {
              const status =
                isRecord(value) && typeof value.status === 'string' ? value.status : null;
              const statusChange =
                isRecord(value) && typeof value.status_change === 'string'
                  ? value.status_change
                  : null;
              return (
                <HStack key={name} gap={2} fontSize='sm'>
                  <Text fontFamily='mono' color='gray.700'>
                    {name}
                  </Text>
                  {statusChange && <Text color='gray.500'>— {statusChange}</Text>}
                  {!statusChange && status && <Text color='gray.500'>— {status}</Text>}
                </HStack>
              );
            })}
          </VStack>
        )}
      </Box>
    </Box>
  );
}

function PreviewLinksSection({
  metadata,
  repositoryId,
  boardProjectId,
  boardRunId,
  baseRunId,
}: {
  metadata: Record<string, unknown> | null;
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
  baseRunId: string | null;
}) {
  const previewsRaw = metadata?.previews;
  const previews = isRecord(previewsRaw) ? previewsRaw : null;

  if (!previews) return null;

  const previewEntries = Object.entries(previews).filter(
    ([, value]) => typeof value === 'string' || isRecord(value),
  );

  if (previewEntries.length === 0) return null;

  const currentRunUrl = routes.run(repositoryId, boardProjectId, boardRunId);
  const baseRunUrl = baseRunId ? routes.run(repositoryId, boardProjectId, baseRunId) : null;

  return (
    <Box>
      <Heading size='md' mb={3}>
        Preview
      </Heading>
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <VStack align='stretch' gap={2}>
          <HStack gap={4} fontSize='sm'>
            <Text>
              Current run:{' '}
              <Link href={currentRunUrl}>
                <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                  {shortId(boardRunId)}
                </Text>
              </Link>
            </Text>
            {baseRunUrl && baseRunId && (
              <Text>
                Base run:{' '}
                <Link href={baseRunUrl}>
                  <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                    {shortId(baseRunId)}
                  </Text>
                </Link>
              </Text>
            )}
          </HStack>
          <Text fontSize='sm' fontWeight='bold' mt={1}>
            Available previews:
          </Text>
          {previewEntries.map(([type, value]) => (
            <HStack key={type} gap={2} fontSize='sm'>
              <Text fontFamily='mono' color='gray.700'>
                {type}
              </Text>
              {typeof value === 'string' && (
                <Text color='gray.500' truncate>
                  — {value}
                </Text>
              )}
              {isRecord(value) && typeof value.path === 'string' && (
                <Text color='gray.500' truncate>
                  — {value.path}
                </Text>
              )}
            </HStack>
          ))}
        </VStack>
      </Box>
    </Box>
  );
}
