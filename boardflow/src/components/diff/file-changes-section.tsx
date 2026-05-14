'use client';

import { Badge, Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';
import { isRecord } from '@/lib/domain/guards';

export interface FileChangesSectionProps {
  summary: ParsedDiffSummary;
  metadata: Record<string, unknown> | null;
}

export function FileChangesSection({ summary, metadata }: FileChangesSectionProps) {
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
