'use client';

import { Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import type { ParsedDiffSummary } from '@/lib/domain/diff-summary';

export interface ChecksSectionProps {
  summary: ParsedDiffSummary;
}

export function ChecksSection({ summary }: ChecksSectionProps) {
  if (summary.checks === null) {
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
