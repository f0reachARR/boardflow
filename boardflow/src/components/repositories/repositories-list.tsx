'use client';

import { Badge, Box, Heading, Table, Text } from '@chakra-ui/react';
import Link from 'next/link';
import { $api } from '@/lib/api/react-query';
import { boardRunStatusColor } from '@/lib/domain/status';
import { formatDate } from '@/lib/format';

export function RepositoriesList() {
  const { data } = $api.useSuspenseQuery('get', '/api/v1/repositories', {
    params: { query: { limit: 50 } },
  });

  const repositories = data?.items ?? [];

  return (
    <Box>
      <Heading size='lg' mb={6}>
        Repositories
      </Heading>

      {repositories.length === 0 ? (
        <Text color='gray.500'>
          No repositories found. Install the BoardFlow GitHub App to get started.
        </Text>
      ) : (
        <Table.Root size='sm' variant='outline'>
          <Table.Header>
            <Table.Row>
              <Table.ColumnHeader>Repository</Table.ColumnHeader>
              <Table.ColumnHeader>Projects</Table.ColumnHeader>
              <Table.ColumnHeader>Latest Status</Table.ColumnHeader>
              <Table.ColumnHeader>Updated</Table.ColumnHeader>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {repositories.map((repo) => (
              <Table.Row
                key={repo.github_repository_id}
                bg={
                  repo.latest_run_status === 'failed'
                    ? 'red.50'
                    : repo.latest_run_status === 'timed_out'
                      ? 'orange.50'
                      : undefined
                }
              >
                <Table.Cell>
                  <Link href={`/repositories/${repo.github_repository_id}`}>
                    <Text
                      color='blue.600'
                      fontWeight='medium'
                      _hover={{ textDecoration: 'underline' }}
                    >
                      {repo.owner}/{repo.name}
                    </Text>
                  </Link>
                </Table.Cell>
                <Table.Cell>{repo.board_project_count}</Table.Cell>
                <Table.Cell>
                  {repo.latest_run_status ? (
                    <Badge colorPalette={boardRunStatusColor(repo.latest_run_status)}>
                      {repo.latest_run_status}
                    </Badge>
                  ) : (
                    <Text color='gray.400' fontSize='sm'>
                      —
                    </Text>
                  )}
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize='sm' color='gray.600'>
                    {formatDate(repo.updated_at)}
                  </Text>
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Root>
      )}
    </Box>
  );
}
