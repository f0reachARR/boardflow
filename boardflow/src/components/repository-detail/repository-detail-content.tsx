'use client';

import { Badge, Box, Heading, HStack, Table, Text, VStack } from '@chakra-ui/react';
import { Key } from 'lucide-react';
import Link from 'next/link';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import { projectStateColor } from '@/lib/domain/status';
import { formatDate } from '@/lib/format';

interface Props {
  repositoryId: string;
}

export function RepositoryDetailContent({ repositoryId }: Props) {
  const { data: repo } = $api.useSuspenseQuery(
    'get',
    '/api/v1/repositories/{github_repository_id}',
    {
      params: { path: { github_repository_id: Number(repositoryId) } },
    },
  );

  const { data: projectsData } = $api.useSuspenseQuery(
    'get',
    '/api/v1/repositories/{github_repository_id}/board-projects',
    {
      params: { path: { github_repository_id: Number(repositoryId) }, query: { limit: 50 } },
    },
  );

  const projects = projectsData?.items ?? [];

  return (
    <Box>
      <Breadcrumb
        items={[
          { label: 'Repositories', href: '/repositories' },
          { label: `${repo.owner}/${repo.name}` },
        ]}
      />
      <VStack align='stretch' gap={6}>
        <Box>
          <HStack gap={2} mb={1}>
            <Heading size='lg'>
              {repo.owner}/{repo.name}
            </Heading>
          </HStack>
          <HStack gap={4} fontSize='sm' color='gray.600'>
            <Text>{repo.board_project_count} projects</Text>
            <Text>Created {formatDate(repo.created_at)}</Text>
            {repo.html_url && (
              <a href={repo.html_url} target='_blank' rel='noopener noreferrer'>
                <Text color='blue.500' _hover={{ textDecoration: 'underline' }}>
                  View on GitHub
                </Text>
              </a>
            )}
          </HStack>
        </Box>

        <Box>
          <Heading size='md' mb={4}>
            Settings
          </Heading>
          <Link href={`/repositories/${repositoryId}/settings/tokens`}>
            <HStack gap={2} color='blue.600' _hover={{ textDecoration: 'underline' }}>
              <Key size={16} />
              <Text fontWeight='medium'>API Tokens</Text>
            </HStack>
          </Link>
        </Box>

        <Box>
          <Heading size='md' mb={4}>
            Board Projects
          </Heading>

          {projects.length === 0 ? (
            <Text color='gray.500'>No board projects found.</Text>
          ) : (
            <Table.Root size='sm' variant='outline'>
              <Table.Header>
                <Table.Row>
                  <Table.ColumnHeader>Project</Table.ColumnHeader>
                  <Table.ColumnHeader>State</Table.ColumnHeader>
                  <Table.ColumnHeader>Path</Table.ColumnHeader>
                  <Table.ColumnHeader>Updated</Table.ColumnHeader>
                </Table.Row>
              </Table.Header>
              <Table.Body>
                {projects.map((project) => (
                  <Table.Row key={project.board_project_id}>
                    <Table.Cell>
                      <Link
                        href={`/repositories/${repositoryId}/boards/${project.board_project_id}`}
                      >
                        <Text
                          color='blue.600'
                          fontWeight='medium'
                          _hover={{ textDecoration: 'underline' }}
                        >
                          {project.display_name}
                        </Text>
                      </Link>
                    </Table.Cell>
                    <Table.Cell>
                      <HStack gap={2}>
                        <Badge colorPalette={projectStateColor(project.state)}>
                          {project.state}
                        </Badge>
                        {project.state === 'timed_out' && (
                          <Text fontSize='xs' color='orange.600'>
                            (中断または未完了の可能性)
                          </Text>
                        )}
                        {project.state === 'detected' && (
                          <Text fontSize='xs' color='gray.500'>
                            (初回Run未完了)
                          </Text>
                        )}
                      </HStack>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize='sm' color='gray.600' fontFamily='mono'>
                        {project.project_path}
                      </Text>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize='sm' color='gray.600'>
                        {formatDate(project.updated_at)}
                      </Text>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Root>
          )}
        </Box>
      </VStack>
    </Box>
  );
}
