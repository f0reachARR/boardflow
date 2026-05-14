'use client';

import { Badge, Box, Heading, HStack, Text } from '@chakra-ui/react';
import Link from 'next/link';
import { diffStatusColor } from '@/lib/domain/status';
import { formatDateTime, shortId } from '@/lib/format';
import { routes } from '@/lib/routes';

export interface DiffHeaderProps {
  status: string;
  baseBoardRunId: string | null;
  boardRunId: string;
  repositoryId: string;
  boardProjectId: string;
  createdAt: string;
}

export function DiffHeader({
  status,
  baseBoardRunId,
  boardRunId,
  repositoryId,
  boardProjectId,
  createdAt,
}: DiffHeaderProps) {
  return (
    <Box>
      <HStack gap={3} mb={2}>
        <Heading size='lg'>Diff</Heading>
        <Badge colorPalette={diffStatusColor(status)} size='lg'>
          {status}
        </Badge>
      </HStack>
      <HStack gap={4} fontSize='sm' color='gray.600'>
        {baseBoardRunId && (
          <Text>
            Compared to:{' '}
            <Link href={routes.run(repositoryId, boardProjectId, baseBoardRunId)}>
              <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                {shortId(baseBoardRunId)}
              </Text>
            </Link>
          </Text>
        )}
        <Text>
          Current:{' '}
          <Link href={routes.run(repositoryId, boardProjectId, boardRunId)}>
            <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
              {shortId(boardRunId)}
            </Text>
          </Link>
        </Text>
        <Text>Created {formatDateTime(createdAt)}</Text>
      </HStack>
    </Box>
  );
}
