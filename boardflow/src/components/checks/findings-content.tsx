'use client';

import { Badge, Box, Heading, HStack, Table, Text } from '@chakra-ui/react';
import Link from 'next/link';
import { Breadcrumb } from '@/components/ui/breadcrumb';
import { $api } from '@/lib/api/react-query';
import { shortId } from '@/lib/format';

function severityColor(severity: string): string {
  switch (severity) {
    case 'error':
      return 'red';
    case 'warning':
      return 'orange';
    case 'notice':
      return 'gray';
    default:
      return 'gray';
  }
}

function locationText(finding: {
  sheet_path?: string | null;
  pcb_layer?: string | null;
  pos_mm?: { x: number; y: number } | null;
}): string {
  if (finding.sheet_path) {
    return `${finding.sheet_path} (schematic)`;
  }
  if (finding.pcb_layer) {
    const pos = finding.pos_mm ? ` @ (${finding.pos_mm.x}, ${finding.pos_mm.y})` : '';
    return `${finding.pcb_layer}${pos}`;
  }
  return '—';
}

interface Props {
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
  checkKind: 'erc' | 'drc';
  severity?: 'error' | 'warning' | 'notice';
}

export function FindingsContent({
  repositoryId,
  boardProjectId,
  boardRunId,
  checkKind,
  severity,
}: Props) {
  const { data: findingsData } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings',
    {
      params: {
        path: { board_run_id: boardRunId, check_kind: checkKind },
        query: severity ? { severity } : undefined,
      },
    },
  );

  const { data: project } = $api.useSuspenseQuery(
    'get',
    '/api/v1/board-projects/{board_project_id}',
    {
      params: { path: { board_project_id: boardProjectId } },
    },
  );

  const findings = findingsData.items;
  const hasMore = findingsData.has_more;

  const basePath = `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${checkKind}`;

  return (
    <Box>
      {project && (
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
            { label: 'Runs', href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs` },
            {
              label: shortId(boardRunId),
              href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`,
            },
            { label: `${checkKind.toUpperCase()} Findings` },
          ]}
        />
      )}
      <Heading size='lg' mb={4}>
        {checkKind.toUpperCase()} Findings
      </Heading>

      {/* Severity filter */}
      <HStack gap={2} mb={4}>
        <Link href={basePath}>
          <Badge colorPalette={!severity ? 'blue' : 'gray'} size='sm' cursor='pointer'>
            All
          </Badge>
        </Link>
        <Link href={`${basePath}?severity=error`}>
          <Badge colorPalette={severity === 'error' ? 'red' : 'gray'} size='sm' cursor='pointer'>
            Errors
          </Badge>
        </Link>
        <Link href={`${basePath}?severity=warning`}>
          <Badge
            colorPalette={severity === 'warning' ? 'orange' : 'gray'}
            size='sm'
            cursor='pointer'
          >
            Warnings
          </Badge>
        </Link>
        <Link href={`${basePath}?severity=notice`}>
          <Badge
            colorPalette={severity === 'notice' ? 'gray' : 'gray'}
            size='sm'
            cursor='pointer'
            variant={severity === 'notice' ? 'solid' : 'outline'}
          >
            Notices
          </Badge>
        </Link>
      </HStack>

      {findings.length === 0 ? (
        <Text color='gray.500'>No findings{severity ? ` with severity "${severity}"` : ''}.</Text>
      ) : (
        <>
          <Table.Root size='sm' variant='outline'>
            <Table.Header>
              <Table.Row>
                <Table.ColumnHeader>Severity</Table.ColumnHeader>
                <Table.ColumnHeader>Rule</Table.ColumnHeader>
                <Table.ColumnHeader>Title</Table.ColumnHeader>
                <Table.ColumnHeader>Message</Table.ColumnHeader>
                <Table.ColumnHeader>Location</Table.ColumnHeader>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {findings.map((finding) => (
                <Table.Row key={finding.id}>
                  <Table.Cell>
                    <Badge colorPalette={severityColor(finding.severity)} size='sm'>
                      {finding.severity}
                    </Badge>
                  </Table.Cell>
                  <Table.Cell>
                    <Text fontSize='sm' fontFamily='mono'>
                      {finding.rule_code}
                    </Text>
                  </Table.Cell>
                  <Table.Cell>
                    <Text fontSize='sm'>{finding.title}</Text>
                  </Table.Cell>
                  <Table.Cell>
                    <Text fontSize='sm' color='gray.600'>
                      {finding.message ?? '—'}
                    </Text>
                  </Table.Cell>
                  <Table.Cell>
                    <Text fontSize='sm' color='gray.600'>
                      {locationText(finding)}
                    </Text>
                  </Table.Cell>
                </Table.Row>
              ))}
            </Table.Body>
          </Table.Root>
          {hasMore && (
            <Text fontSize='sm' color='gray.500' mt={3}>
              More results available. Showing first {findings.length} findings.
            </Text>
          )}
        </>
      )}
    </Box>
  );
}
