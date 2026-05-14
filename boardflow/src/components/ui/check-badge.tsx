import { Badge, Text } from '@chakra-ui/react';
import { checkStatusColor } from '@/lib/domain/status';

export function CheckBadge({ status }: { status: string | null | undefined }) {
  if (!status)
    return (
      <Text color='gray.400' fontSize='sm'>
        —
      </Text>
    );
  return (
    <Badge colorPalette={checkStatusColor(status)} size='sm'>
      {status}
    </Badge>
  );
}
