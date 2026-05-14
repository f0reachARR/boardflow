'use client';

import { Box, Text } from '@chakra-ui/react';

export interface DiffStatusMessageProps {
  status: string;
  errorMessage: string | null;
}

export function DiffStatusMessage({ status, errorMessage }: DiffStatusMessageProps) {
  if (status === 'no_baseline') {
    return (
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <Text fontSize='sm' color='gray.500'>
          This is the first completed run. No baseline available for comparison.
        </Text>
      </Box>
    );
  }

  if (status === 'unavailable') {
    return (
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <Text fontSize='sm' color='gray.500'>
          Diff data is not available. The baseline or current run may be missing required data.
        </Text>
      </Box>
    );
  }

  if (status === 'failed') {
    return (
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <Text fontSize='sm' color='red.500'>
          {errorMessage ?? 'Diff computation failed.'}
        </Text>
      </Box>
    );
  }

  return null;
}
