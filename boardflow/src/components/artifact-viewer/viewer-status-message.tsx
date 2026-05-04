'use client';

import { Box, Text } from '@chakra-ui/react';

interface ViewerStatusMessageProps {
  status: string;
  viewerName: string;
}

export function ViewerStatusMessage({ status, viewerName }: ViewerStatusMessageProps) {
  const message = (() => {
    switch (status) {
      case 'missing':
        return `${viewerName} sources are not available.`;
      case 'failed':
        return `Failed to generate ${viewerName} sources.`;
      case 'skipped':
        return `${viewerName} generation was skipped.`;
      default:
        return `${viewerName} is not available.`;
    }
  })();

  return (
    <Box p={4} borderWidth='1px' borderRadius='md' bg='gray.50'>
      <Text fontSize='sm' color='gray.500'>
        {message}
      </Text>
    </Box>
  );
}
