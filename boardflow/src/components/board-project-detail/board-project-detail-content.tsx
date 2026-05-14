'use client';

import { Badge, Box, Heading, HStack, Table, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { CheckBadge } from '@/components/ui/check-badge';
import { $api } from '@/lib/api/react-query';
import { boardRunStatusColor } from '@/lib/domain/status';
import { formatDateTime, shortSha } from '@/lib/format';
import { routes } from '@/lib/routes';

interface Props {
  repositoryId: string;
  boardProjectId: string;
}

export function BoardProjectDetailContent({ repositoryId, boardProjectId }: Props) {
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
      params: { path: { board_project_id: boardProjectId }, query: { limit: 5 } },
    },
  );

  const recentRuns = runsData?.items ?? [];

  return (
    <Box>
      <Breadcrumb
        items={[
          { label: 'Repositories', href: routes.repositories() },
          {
            label: `${project.repository.owner}/${project.repository.name}`,
            href: routes.repository(repositoryId),
          },
          { label: project.display_name },
        ]}
      />
      <VStack align='stretch' gap={6}>
        <Box>
          <HStack gap={2} mb={1}>
            <Heading size='lg'>{project.display_name}</Heading>
            <Badge colorPalette={project.state === 'completed' ? 'green' : 'gray'}>
              {project.state}
            </Badge>
          </HStack>
          <Text fontSize='sm' color='gray.600' fontFamily='mono'>
            {project.project_path}
          </Text>
        </Box>

        <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
          <VStack align='stretch' gap={3}>
            <HStack justify='space-between'>
              <Text fontWeight='medium'>Repository</Text>
              <Link href={routes.repository(repositoryId)}>
                <Text color='blue.600' _hover={{ textDecoration: 'underline' }}>
                  {project.repository.owner}/{project.repository.name}
                </Text>
              </Link>
            </HStack>
            <HStack justify='space-between'>
              <Text fontWeight='medium'>Project Directory</Text>
              <Text fontFamily='mono' fontSize='sm'>
                {project.project_dir}
              </Text>
            </HStack>
            {project.issue_url && (
              <HStack justify='space-between'>
                <Text fontWeight='medium'>Issue</Text>
                <a href={project.issue_url} target='_blank' rel='noopener noreferrer'>
                  <Text color='blue.600' _hover={{ textDecoration: 'underline' }}>
                    #{project.issue_number}
                  </Text>
                </a>
              </HStack>
            )}
            {project.latest_completed_run_id && (
              <HStack justify='space-between'>
                <Text fontWeight='medium'>Latest Completed Run</Text>
                <Link
                  href={routes.run(repositoryId, boardProjectId, project.latest_completed_run_id)}
                >
                  <Text color='blue.600' _hover={{ textDecoration: 'underline' }}>
                    {project.latest_completed_run_id}
                  </Text>
                </Link>
              </HStack>
            )}
            <HStack justify='space-between'>
              <Text fontWeight='medium'>Created</Text>
              <Text fontSize='sm' color='gray.600'>
                {formatDateTime(project.created_at)}
              </Text>
            </HStack>
          </VStack>
        </Box>

        <Box>
          <Heading size='md' mb={3}>
            Recent Runs
          </Heading>
          {recentRuns.length === 0 ? (
            <Text color='gray.500' fontSize='sm'>
              No runs yet.
            </Text>
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
                {recentRuns.map((run) => (
                  <Table.Row key={run.board_run_id}>
                    <Table.Cell>
                      <Link href={routes.run(repositoryId, boardProjectId, run.board_run_id)}>
                        <Badge colorPalette={boardRunStatusColor(run.status)}>{run.status}</Badge>
                      </Link>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontFamily='mono' fontSize='sm'>
                        {shortSha(run.commit_sha)}
                      </Text>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize='sm'>{run.branch}</Text>
                    </Table.Cell>
                    <Table.Cell>
                      <HStack gap={1}>
                        <CheckBadge status={run.erc_status} />
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
                        <CheckBadge status={run.drc_status} />
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
                        {formatDateTime(run.created_at)}
                      </Text>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Root>
          )}
        </Box>

        <Box>
          <Link href={routes.runs(repositoryId, boardProjectId)}>
            <Box
              display='inline-flex'
              alignItems='center'
              px={4}
              py={2}
              borderRadius='md'
              bg='blue.600'
              color='white'
              fontWeight='medium'
              fontSize='sm'
              _hover={{ bg: 'blue.700' }}
            >
              View All Runs
            </Box>
          </Link>
        </Box>
      </VStack>
    </Box>
  );
}
