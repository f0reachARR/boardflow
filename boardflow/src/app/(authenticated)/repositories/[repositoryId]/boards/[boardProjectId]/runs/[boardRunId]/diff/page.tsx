import { Badge, Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import type { DiffResponse, DiffSummary } from '@/lib/api/schema-types';
import { createServerClient } from '@/lib/api/server';

function diffStatusColor(status: string): string {
  switch (status) {
    case 'ready':
      return 'green';
    case 'no_baseline':
      return 'gray';
    case 'unavailable':
      return 'orange';
    case 'failed':
      return 'red';
    default:
      return 'gray';
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isFileChanges(
  v: unknown,
): v is { added: number; removed: number; changed: number; unchanged: number } {
  return (
    isRecord(v) &&
    typeof v.added === 'number' &&
    typeof v.removed === 'number' &&
    typeof v.changed === 'number' &&
    typeof v.unchanged === 'number'
  );
}

function isBomChanges(v: unknown): v is { added: number; removed: number; changed: number } {
  return (
    isRecord(v) &&
    typeof v.added === 'number' &&
    typeof v.removed === 'number' &&
    typeof v.changed === 'number'
  );
}

function isCheckEntry(
  v: unknown,
): v is { status_change: string; error_delta: number; warning_delta: number } {
  return (
    isRecord(v) &&
    typeof v.status_change === 'string' &&
    typeof v.error_delta === 'number' &&
    typeof v.warning_delta === 'number'
  );
}

function isArtifactChanges(v: unknown): v is { added: number; removed: number; changed: number } {
  return (
    isRecord(v) &&
    typeof v.added === 'number' &&
    typeof v.removed === 'number' &&
    typeof v.changed === 'number'
  );
}

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>;
}

export default async function DiffPage({ params }: Props) {
  const { repositoryId, boardProjectId, boardRunId } = await params;
  const client = await createServerClient();

  const [diffRes, projectRes] = await Promise.all([
    client.GET('/api/v1/board-runs/{board_run_id}/diff', {
      params: { path: { board_run_id: boardRunId } },
    }),
    client.GET('/api/v1/board-projects/{board_project_id}', {
      params: { path: { board_project_id: boardProjectId } },
    }),
  ]);

  if (diffRes.error) {
    if (diffRes.error.error?.code === 'not_found') {
      notFound();
    }
    const errorMessage = diffRes.error.error?.message ?? 'Failed to load diff data.';
    return (
      <Box p={6}>
        <Heading size='lg' mb={4}>
          Diff
        </Heading>
        <Box borderWidth='1px' borderRadius='md' p={4} bg='red.50'>
          <Text color='red.600'>{errorMessage}</Text>
        </Box>
      </Box>
    );
  }

  const diff: DiffResponse = diffRes.data;
  const project = projectRes.data;

  return (
    <Box>
      {project && (
        <Breadcrumb
          items={[
            { label: 'Repositories', href: '/repositories' },
            {
              label: `${project.repository.owner}/${project.repository.name}`,
              href: `/repositories/${repositoryId}`,
            },
            {
              label: project.display_name,
              href: `/repositories/${repositoryId}/boards/${boardProjectId}`,
            },
            { label: 'Runs', href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs` },
            {
              label: boardRunId.slice(0, 8),
              href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`,
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
                <Link
                  href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${diff.base_board_run_id}`}
                >
                  <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                    {diff.base_board_run_id.slice(0, 8)}
                  </Text>
                </Link>
              </Text>
            )}
            <Text>
              Current:{' '}
              <Link
                href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`}
              >
                <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                  {boardRunId.slice(0, 8)}
                </Text>
              </Link>
            </Text>
            <Text>Created {new Date(diff.created_at).toLocaleString()}</Text>
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

        {diff.status === 'ready' && diff.summary != null && (
          <>
            <FileChangesSection
              summary={diff.summary as DiffSummary}
              metadata={diff.metadata ?? null}
            />
            <BomChangesSection
              summary={diff.summary as DiffSummary}
              metadata={diff.metadata ?? null}
            />
            <ChecksSection summary={diff.summary as DiffSummary} />
            <ArtifactChangesSection
              summary={diff.summary as DiffSummary}
              metadata={diff.metadata ?? null}
            />
            <PreviewLinksSection
              metadata={diff.metadata ?? null}
              repositoryId={repositoryId}
              boardProjectId={boardProjectId}
              boardRunId={boardRunId}
              baseRunId={diff.base_board_run_id ?? null}
            />
          </>
        )}
      </VStack>
    </Box>
  );
}

function FileChangesSection({
  summary,
  metadata,
}: {
  summary: DiffSummary;
  metadata: Record<string, unknown> | null;
}) {
  if (!isFileChanges(summary.file_changes)) {
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

  const { added, removed, changed, unchanged } = summary.file_changes;

  // metadata.file_hashes is an Object map: { "path/to/file": { "hash": "..." } }
  const fileHashesRaw = metadata?.file_hashes;
  const fileHashes = isRecord(fileHashesRaw) ? fileHashesRaw : null;
  const fileCount = fileHashes ? Object.keys(fileHashes).length : null;

  // Try to extract file paths that have a "status" field indicating change
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
  summary: DiffSummary;
  metadata: Record<string, unknown> | null;
}) {
  if (!isBomChanges(summary.bom_changes)) {
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

  const { added, removed, changed } = summary.bom_changes;

  // metadata.bom_summary structure is not strictly defined; safely check if it's an object
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

function ChecksSection({ summary }: { summary: DiffSummary }) {
  if (!isRecord(summary.checks)) {
    return (
      <Box>
        <Heading size='md' mb={3}>
          ERC/DRC Checks
        </Heading>
        <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
          <Text fontSize='sm' color='gray.500'>
            Data format not recognized
          </Text>
        </Box>
      </Box>
    );
  }

  const validChecks = Object.entries(summary.checks).filter(
    (
      entry,
    ): entry is [string, { status_change: string; error_delta: number; warning_delta: number }] =>
      isCheckEntry(entry[1]),
  );
  if (validChecks.length === 0) return null;

  return (
    <Box>
      <Heading size='md' mb={3}>
        ERC/DRC Checks
      </Heading>
      <HStack gap={4} flexWrap='wrap'>
        {validChecks.map(([kind, check]) => (
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
  summary: DiffSummary;
  metadata: Record<string, unknown> | null;
}) {
  if (!isArtifactChanges(summary.artifacts)) {
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

  const { added, removed, changed } = summary.artifacts;

  // metadata.artifacts_summary: Object with artifact names as keys and status info as values
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

  const currentRunUrl = `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`;
  const baseRunUrl = baseRunId
    ? `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${baseRunId}`
    : null;

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
                  {boardRunId.slice(0, 8)}
                </Text>
              </Link>
            </Text>
            {baseRunUrl && baseRunId && (
              <Text>
                Base run:{' '}
                <Link href={baseRunUrl}>
                  <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                    {baseRunId.slice(0, 8)}
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
