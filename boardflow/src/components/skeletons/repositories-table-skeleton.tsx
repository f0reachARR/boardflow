import { Box, Heading, Table, Skeleton } from "@chakra-ui/react"

export function RepositoriesTableSkeleton() {
  return (
    <Box>
      <Heading size="lg" mb={6}>Repositories</Heading>
      <Table.Root size="sm" variant="outline">
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeader>Repository</Table.ColumnHeader>
            <Table.ColumnHeader>Projects</Table.ColumnHeader>
            <Table.ColumnHeader>Latest Status</Table.ColumnHeader>
            <Table.ColumnHeader>Updated</Table.ColumnHeader>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {Array.from({ length: 5 }).map((_, i) => (
            <Table.Row key={i}>
              <Table.Cell><Skeleton height="20px" width="200px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="30px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="80px" /></Table.Cell>
              <Table.Cell><Skeleton height="20px" width="100px" /></Table.Cell>
            </Table.Row>
          ))}
        </Table.Body>
      </Table.Root>
    </Box>
  )
}
