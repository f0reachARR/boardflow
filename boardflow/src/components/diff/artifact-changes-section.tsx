'use client';

import { Badge, Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';
import { isRecord } from '@/lib/domain/guards';

export interface ArtifactChangesSectionProps {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}

export function ArtifactChangesSection({ summary, metadata }: ArtifactChangesSectionProps) {
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
