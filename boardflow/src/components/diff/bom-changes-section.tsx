'use client';

import { Badge, Box, Heading, HStack, Text } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';

export interface BomChangesSectionProps {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}

export function BomChangesSection({ summary, metadata }: BomChangesSectionProps) {
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
