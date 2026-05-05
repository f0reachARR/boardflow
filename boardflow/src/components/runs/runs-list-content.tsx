'use client';

import { Badge, Box, Heading, HStack, Table, Text } from '@chakra-ui/react';
import Link from 'next/link';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';

function statusColor(status: string): string {
  switch (status) {
    case 'completed':
      return 'green';
    case 'failed':
      return 'red';
    case 'timed_out':
      return 'orange';
    case 'created':
    case 'uploading':
    case 'importing':
      return 'blue';
    default:
      return 'gray';
  }
}

function checkBadge(status: string | null | undefined) {
  if (!status)
    return (
      <Text color='gray.400' fontSize='sm'>
        —
      </Text>
    );
  const color = status === 'passed' ? 'green' : status === 'failed' ? 'red' : 'gray';
  return (
    <Badge colorPalette={color} size='sm'>
      {status}
    </Badge>
  );
}

interface Props {
  repositoryId: string;
  boardProjectId: string;
}

export function RunsListContent({ repositoryId, boardProjectId }: Props) {
  const { data: project } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-projects/{board_project_id}',
    {
      params: { path: { board_project_id: boardProjectId } },
    },
  );

  const { data: runsData } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-projects/{board_project_id}/board-runs',
    {
      params: { path: { board_project_id: boardProjectId }, query: { limit: 50 } },
    },
  );

  const runs = runsData?.items ?? [];

  return (
    <Box>
      <Breadcrumb
        items={[
          { label: 'Repositories', href: '/repositories' },
          {
            label: `${project.repository.owner}/${project.repository.name}`,
            href: `/repositories/${repositoryId}`,
          },
          {
            label: project.display_name,
            href: `/repositories/${repositoryId}/boards/${boardProjectId}`,
          },
          { label: 'Runs' },
        ]}
      />
      <Heading size='lg' mb={6}>
        Runs
      </Heading>

      {runs.length === 0 ? (
        <Text color='gray.500'>No runs yet.</Text>
      ) : (
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
            {runs.map((run) => (
              <Table.Row key={run.board_run_id}>
                <Table.Cell>
                  <Link
                    href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${run.board_run_id}`}
                  >
                    <Badge colorPalette={statusColor(run.status)}>{run.status}</Badge>
                  </Link>
                </Table.Cell>
                <Table.Cell>
                  <Text fontFamily='mono' fontSize='sm'>
                    {run.commit_sha.slice(0, 7)}
                  </Text>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize='sm'>{run.branch}</Text>
                </Table.Cell>
                <Table.Cell>
                  <HStack gap={1}>
                    {checkBadge(run.erc_status)}
                    {run.erc_errors != null && run.erc_errors > 0 && (
                      <Text fontSize='xs' color='red.500'>
                        {run.erc_errors}E
                      </Text>
                    )}
                    {run.erc_warnings != null && run.erc_warnings > 0 && (
                      <Text fontSize='xs' color='orange.500'>
                        {run.erc_warnings}W
                      </Text>
                    )}
                  </HStack>
                </Table.Cell>
                <Table.Cell>
                  <HStack gap={1}>
                    {checkBadge(run.drc_status)}
                    {run.drc_errors != null && run.drc_errors > 0 && (
                      <Text fontSize='xs' color='red.500'>
                        {run.drc_errors}E
                      </Text>
                    )}
                    {run.drc_warnings != null && run.drc_warnings > 0 && (
                      <Text fontSize='xs' color='orange.500'>
                        {run.drc_warnings}W
                      </Text>
                    )}
                  </HStack>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize='sm' color='gray.600'>
                    {new Date(run.created_at).toLocaleString()}
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
