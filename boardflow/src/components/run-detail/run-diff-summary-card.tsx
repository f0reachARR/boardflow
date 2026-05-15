import { Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import type { DiffResponse } from '@/lib/api/schema-types';
import { parseDiffSummary } from '@/lib/domain/diff-summary';
import { shortId } from '@/lib/format';
import { routes } from '@/lib/routes';

interface RunDiffSummaryCardProps {
  diff: DiffResponse | null;
  diffErrorMessage: string | null;
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
}

export function RunDiffSummaryCard({
  diff,
  diffErrorMessage,
  repositoryId,
  boardProjectId,
  boardRunId,
}: RunDiffSummaryCardProps) {
  if (diff === null && diffErrorMessage === null) {
    return null;
  }

  return (
    <>
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
                          <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                            {shortId(diff.base_board_run_id)}
                          </Text>
                        </Link>
                      </Text>
                    )}
                    {summary.fileChanges ? (
                      <HStack gap={4} fontSize='sm'>
                        <Text>
                          Files: +{summary.fileChanges.added} -{summary.fileChanges.removed} ~
                          {summary.fileChanges.changed} ({summary.fileChanges.unchanged} unchanged)
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
                    {summary.checks === null ? (
                      <Text fontSize='sm' color='gray.500'>
                        Checks: data format not recognized
                      </Text>
                    ) : (
                      summary.checks.length > 0 && (
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
                      )
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
    </>
  );
}
