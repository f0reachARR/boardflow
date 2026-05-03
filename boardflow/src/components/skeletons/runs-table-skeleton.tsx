import { Box, Heading, Skeleton, Table } from "@chakra-ui/react"

export function RunsTableSkeleton() {
  return (
    <Box>
      <Skeleton height="20px" width="300px" mb={4} />
      <Heading size="lg" mb={6}>Runs</Heading>
      <Table.Root size="sm" variant="outline">
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
          {Array.from({ length: 5 }).map((_, i) => (
            <Table.Row key={i}>
              <Table.Cell><Skeleton height="20px" width="80px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="70px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="100px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="60px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="60px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="100px" /></Table.Cell>
            </Table.Row>
          ))}
        </Table.Body>
      </Table.Root>
    </Box>
  )
}
