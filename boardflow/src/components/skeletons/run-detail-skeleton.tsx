import { Box, VStack, HStack, Skeleton, Heading, Table } from "@chakra-ui/react"

export function RunDetailSkeleton() {
  return (
    <Box>
      <Skeleton height="20px" width="400px" mb={4} />
      <VStack align="stretch" gap={6}>
        {/* Header */}
        <Box>
          <HStack gap={3} mb={2}>
            <Skeleton height="32px" width="150px" />
            <Skeleton height="24px" width="80px" />
          </HStack>
          <HStack gap={4}>
            <Skeleton height="16px" width="70px" />
            <Skeleton height="16px" width="80px" />
            <Skeleton height="16px" width="120px" />
          </HStack>
        </Box>
        {/* Checks */}
        <Box>
          <Heading size="md" mb={3}>Checks</Heading>
          <HStack gap={4}>
            {Array.from({ length: 2 }).map((_, i) => (
              <Box key={i} borderWidth="1px" borderRadius="md" p={4} minW="200px">
                <HStack justify="space-between" mb={2}>
                  <Skeleton height="16px" width="50px" />
                  <Skeleton height="20px" width="60px" />
                </HStack>
                <Skeleton height="14px" width="100px" />
              </Box>
            ))}
          </HStack>
        </Box>
        {/* Artifact Summary */}
        <Box>
          <Heading size="md" mb={3}>Artifact Summary</Heading>
          <HStack gap={4}>
            <Skeleton height="20px" width="100px" />
            <Skeleton height="20px" width="80px" />
          </HStack>
        </Box>
        {/* Artifacts Table */}
        <Box>
          <Heading size="md" mb={3}>Artifacts</Heading>
          <Table.Root size="sm" variant="outline">
            <Table.Header>
              <Table.Row>
                <Table.ColumnHeader>Type</Table.ColumnHeader>
                <Table.ColumnHeader>Status</Table.ColumnHeader>
                <Table.ColumnHeader>Filename</Table.ColumnHeader>
                <Table.ColumnHeader>Size</Table.ColumnHeader>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {Array.from({ length: 4 }).map((_, i) => (
                <Table.Row key={i}>
                  <Table.Cell><Skeleton height="20px" width="80px" /></Table.Cell>
                  <Table.Cell><Skeleton height="20px" width="70px" /></Table.Cell>
                  <Table.Cell><Skeleton height="20px" width="150px" /></Table.Cell>
                  <Table.Cell><Skeleton height="20px" width="60px" /></Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </Table.Root>
        </Box>
      </VStack>
    </Box>
  )
}
