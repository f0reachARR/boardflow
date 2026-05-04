import { Box, Heading, HStack, Skeleton, Table, VStack } from '@chakra-ui/react';

export function BoardProjectDetailSkeleton() {
  return (
    <Box>
      <Skeleton height='20px' width='300px' mb={4} />
      <VStack align='stretch' gap={6}>
        <Box>
          <HStack gap={2} mb={1}>
            <Skeleton height='32px' width='200px' />
            <Skeleton height='24px' width='80px' />
          </HStack>
          <Skeleton height='16px' width='250px' />
        </Box>
        <Box borderWidth='1px' borderRadius='md' p={4}>
          <VStack align='stretch' gap={3}>
            {Array.from({ length: 4 }).map((_, i) => (
              <HStack key={i} justify='space-between'>
                <Skeleton height='16px' width='100px' />
                <Skeleton height='16px' width='150px' />
              </HStack>
            ))}
          </VStack>
        </Box>
        <Box>
          <Heading size='md' mb={3}>
            Recent Runs
          </Heading>
          <Table.Root size='sm' variant='outline'>
            <Table.Header>
              <Table.Row>
                <Table.ColumnHeader>Status</Table.ColumnHeader>
                <Table.ColumnHeader>Commit</Table.ColumnHeader>
                <Table.ColumnHeader>Branch</Table.ColumnHeader>
                <Table.ColumnHeader>ERC</Table.ColumnHeader>
                <Table.ColumnHeader>DRC</Table.ColumnHeader>
                <Table.ColumnHeader>Created</Table.ColumnHeader>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {Array.from({ length: 3 }).map((_, i) => (
                <Table.Row key={i}>
                  <Table.Cell>
                    <Skeleton height='20px' width='80px' />
                  </Table.Cell>
                  <Table.Cell>
                    <Skeleton height='20px' width='70px' />
                  </Table.Cell>
                  <Table.Cell>
                    <Skeleton height='20px' width='100px' />
                  </Table.Cell>
                  <Table.Cell>
                    <Skeleton height='20px' width='60px' />
                  </Table.Cell>
                  <Table.Cell>
                    <Skeleton height='20px' width='60px' />
                  </Table.Cell>
                  <Table.Cell>
                    <Skeleton height='20px' width='100px' />
                  </Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </Table.Root>
        </Box>
      </VStack>
    </Box>
  );
}
