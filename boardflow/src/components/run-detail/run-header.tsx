import { Badge, Box, Heading, HStack, Text } from '@chakra-ui/react';
import type { BoardRunDetail } from '@/lib/api/schema-types';
import { boardRunStatusColor } from '@/lib/domain/status';
import { formatDateTime, shortSha } from '@/lib/format';

interface RunHeaderProps {
  run: BoardRunDetail;
}

export function RunHeader({ run }: RunHeaderProps) {
  return (
    <Box>
      <HStack gap={3} mb={2}>
        <Heading size='lg'>Run {run.board_run_id}</Heading>
        <Badge colorPalette={boardRunStatusColor(run.status)} size='lg'>
          {run.status}
        </Badge>
      </HStack>
      <HStack gap={4} fontSize='sm' color='gray.600'>
        <Text fontFamily='mono'>{shortSha(run.commit_sha)}</Text>
        <Text>{run.branch}</Text>
        <Text>Created {formatDateTime(run.created_at)}</Text>
        {run.completed_at && <Text>Completed {formatDateTime(run.completed_at)}</Text>}
      </HStack>
    </Box>
  );
}
