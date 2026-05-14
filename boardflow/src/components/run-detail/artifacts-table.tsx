import { Badge, Box, Heading, Table, Text } from '@chakra-ui/react';
import type { Artifact } from '@/lib/api/schema-types';
import { artifactStatusColor } from '@/lib/domain/status';
import { formatBytes } from '@/lib/format';

interface ArtifactsTableProps {
  artifacts: Artifact[];
}

export function ArtifactsTable({ artifacts }: ArtifactsTableProps) {
  if (artifacts.length === 0) {
    return null;
  }

  return (
    <Box>
      <Heading size='md' mb={3}>
        Artifacts
      </Heading>
      <Table.Root size='sm' variant='outline'>
        <Table.Header>
          <Table.Row>
            <Table.ColumnHeader>Type</Table.ColumnHeader>
            <Table.ColumnHeader>Status</Table.ColumnHeader>
            <Table.ColumnHeader>Filename</Table.ColumnHeader>
            <Table.ColumnHeader>Size</Table.ColumnHeader>
          </Table.Row>
        </Table.Header>
        <Table.Body>
          {artifacts.map((artifact, idx) => (
            <Table.Row key={artifact.artifact_id ?? idx}>
              <Table.Cell>
                <Text fontSize='sm'>{artifact.type}</Text>
              </Table.Cell>
              <Table.Cell>
                <Badge colorPalette={artifactStatusColor(artifact.status)}>{artifact.status}</Badge>
              </Table.Cell>
              <Table.Cell>
                <Text fontSize='sm' fontFamily='mono'>
                  {artifact.filename ?? artifact.status_reason ?? '—'}
                </Text>
              </Table.Cell>
              <Table.Cell>
                <Text fontSize='sm' color='gray.600'>
                  {artifact.size_bytes ? formatBytes(artifact.size_bytes) : '—'}
                </Text>
              </Table.Cell>
            </Table.Row>
          ))}
        </Table.Body>
      </Table.Root>
    </Box>
  );
}
