import { Box, Heading, VStack, HStack, Skeleton, Table } from "@chakra-ui/react"

export function RepositoryDetailSkeleton() {
  return (
    <Box>
      <Skeleton height="20px" width="200px" mb={4} />
      <VStack align="stretch" gap={6}>
        <Box>
          <HStack gap={2} mb={1}>
            <Skeleton height="32px" width="250px" />
          </HStack>
          <HStack gap={4}>
            <Skeleton height="16px" width="80px" />
            <Skeleton height="16px" width="120px" />
          </HStack>
        </Box>
        <Box>
          <Heading size="md" mb={4}>Board Projects</Heading>
          <Table.Root size="sm" variant="outline">
            <Table.Header>
              <Table.Row>
                <Table.ColumnHeader>Project</Table.ColumnHeader>
                <Table.ColumnHeader>State</Table.ColumnHeader>
                <Table.ColumnHeader>Path</Table.ColumnHeader>
                <Table.ColumnHeader>Updated</Table.ColumnHeader>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {Array.from({ length: 3 }).map((_, i) => (
                <Table.Row key={i}>
                  <Table.Cell><Skeleton height="20px" width="150px" /></Table.Cell>
                  <Table.Cell><Skeleton height="20px" width="80px" /></Table.Cell>
                  <Table.Cell><Skeleton height="20px" width="180px" /></Table.Cell>
                  <Table.Cell><Skeleton height="20px" width="100px" /></Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </Table.Root>
        </Box>
      </VStack>
    </Box>
  )
}
