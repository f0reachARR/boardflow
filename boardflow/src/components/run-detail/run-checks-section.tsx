import { Badge, Box, Heading, HStack, Text } from '@chakra-ui/react';
import Link from 'next/link';
import type { CheckInfo } from '@/lib/api/schema-types';
import { checkStatusColor } from '@/lib/domain/status';
import { routes } from '@/lib/routes';

interface RunChecksSectionProps {
  checks: CheckInfo[];
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
}

export function RunChecksSection({
  checks,
  repositoryId,
  boardProjectId,
  boardRunId,
}: RunChecksSectionProps) {
  if (checks.length === 0) {
    return null;
  }

  return (
    <Box>
      <Heading size='md' mb={3}>
        Checks
      </Heading>
      <HStack gap={4}>
        {checks.map((check) => (
          <Box key={check.kind} borderWidth='1px' borderRadius='md' p={4} bg='white' minW='200px'>
            <HStack justify='space-between' mb={2}>
              <Text fontWeight='bold' textTransform='uppercase'>
                {check.kind}
              </Text>
              <Badge colorPalette={checkStatusColor(check.status)}>{check.status}</Badge>
            </HStack>
            <HStack gap={3} fontSize='sm'>
              {check.error_count > 0 && <Text color='red.500'>{check.error_count} errors</Text>}
              {check.warning_count > 0 && (
                <Text color='orange.500'>{check.warning_count} warnings</Text>
              )}
              {check.notice_count > 0 && <Text color='gray.500'>{check.notice_count} notices</Text>}
              {check.error_count === 0 && check.warning_count === 0 && check.notice_count === 0 && (
                <Text color='green.500'>No issues</Text>
              )}
            </HStack>
            {check.error_count + check.warning_count + check.notice_count > 0 && (
              <Link href={routes.runChecks(repositoryId, boardProjectId, boardRunId, check.kind)}>
                <Text
                  color='blue.600'
                  fontSize='sm'
                  mt={2}
                  _hover={{ textDecoration: 'underline' }}
                >
                  View {check.error_count + check.warning_count + check.notice_count} findings
                </Text>
              </Link>
            )}
          </Box>
        ))}
      </HStack>
    </Box>
  );
}
