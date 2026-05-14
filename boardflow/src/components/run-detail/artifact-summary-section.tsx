import { Badge, Box, Heading, HStack } from '@chakra-ui/react';
import type { ArtifactSummary } from '@/lib/api/schema-types';

interface ArtifactSummarySectionProps {
  artifactSummary: ArtifactSummary;
}

export function ArtifactSummarySection({ artifactSummary }: ArtifactSummarySectionProps) {
  return (
    <Box>
      <Heading size='md' mb={3}>
        Artifact Summary
      </Heading>
      <HStack gap={4} fontSize='sm'>
        <Badge colorPalette='green'>{artifactSummary.available} available</Badge>
        {artifactSummary.missing > 0 && (
          <Badge colorPalette='orange'>{artifactSummary.missing} missing</Badge>
        )}
        {artifactSummary.failed > 0 && (
          <Badge colorPalette='red'>{artifactSummary.failed} failed</Badge>
        )}
        {artifactSummary.skipped > 0 && (
          <Badge colorPalette='gray'>{artifactSummary.skipped} skipped</Badge>
        )}
      </HStack>
    </Box>
  );
}
