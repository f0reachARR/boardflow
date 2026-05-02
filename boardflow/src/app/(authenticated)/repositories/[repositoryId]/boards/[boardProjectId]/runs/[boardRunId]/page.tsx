import { Box, Heading, Text, VStack, HStack, Badge, Table } from "@chakra-ui/react"
import { notFound } from "next/navigation"
import { createServerClient } from "@/lib/api/server"
import type { Artifact, ViewerEntry } from "@/lib/api/schema"

function statusColor(status: string): string {
  switch (status) {
    case "completed":
      return "green"
    case "failed":
      return "red"
    case "timed_out":
      return "orange"
    default:
      return "gray"
  }
}

function checkStatusColor(status: string): string {
  switch (status) {
    case "passed":
      return "green"
    case "failed":
      return "red"
    default:
      return "gray"
  }
}

function artifactStatusColor(status: string): string {
  switch (status) {
    case "available":
      return "green"
    case "missing":
      return "orange"
    case "failed":
      return "red"
    case "skipped":
      return "gray"
    default:
      return "gray"
  }
}

function viewerStatusColor(status: string): string {
  switch (status) {
    case "available":
      return "green"
    case "partial":
      return "yellow"
    case "missing":
      return "orange"
    case "failed":
      return "red"
    case "skipped":
      return "gray"
    default:
      return "gray"
  }
}

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>
}

export default async function RunDetailPage({ params }: Props) {
  const { boardRunId } = await params
  const client = await createServerClient()

  const [runRes, artifactsRes, viewerRes] = await Promise.all([
    client.GET("/api/v1/board-runs/{board_run_id}", {
      params: { path: { board_run_id: boardRunId } },
    }),
    client.GET("/api/v1/board-runs/{board_run_id}/artifacts", {
      params: { path: { board_run_id: boardRunId } },
    }),
    client.GET("/api/v1/board-runs/{board_run_id}/viewer-sources", {
      params: { path: { board_run_id: boardRunId } },
    }),
  ])

  if (runRes.error) {
    notFound()
  }

  const run = runRes.data!
  const artifacts: Artifact[] = artifactsRes.data?.items ?? []
  const viewers: Record<string, ViewerEntry> = viewerRes.data?.viewers ?? {}

  return (
    <Box>
      <VStack align="stretch" gap={6}>
        {/* Header */}
        <Box>
          <HStack gap={3} mb={2}>
            <Heading size="lg">Run {run.board_run_id}</Heading>
            <Badge colorPalette={statusColor(run.status)} size="lg">
              {run.status}
            </Badge>
          </HStack>
          <HStack gap={4} fontSize="sm" color="gray.600">
            <Text fontFamily="mono">{run.commit_sha.slice(0, 7)}</Text>
            <Text>{run.branch}</Text>
            <Text>Created {new Date(run.created_at).toLocaleString()}</Text>
            {run.completed_at && (
              <Text>Completed {new Date(run.completed_at).toLocaleString()}</Text>
            )}
          </HStack>
        </Box>

        {/* Checks */}
        {run.checks.length > 0 && (
          <Box>
            <Heading size="md" mb={3}>Checks</Heading>
            <HStack gap={4}>
              {run.checks.map((check) => (
                <Box
                  key={check.kind}
                  borderWidth="1px"
                  borderRadius="md"
                  p={4}
                  bg="white"
                  minW="200px"
                >
                  <HStack justify="space-between" mb={2}>
                    <Text fontWeight="bold" textTransform="uppercase">
                      {check.kind}
                    </Text>
                    <Badge colorPalette={checkStatusColor(check.status)}>
                      {check.status}
                    </Badge>
                  </HStack>
                  <HStack gap={3} fontSize="sm">
                    {check.error_count > 0 && (
                      <Text color="red.500">{check.error_count} errors</Text>
                    )}
                    {check.warning_count > 0 && (
                      <Text color="orange.500">{check.warning_count} warnings</Text>
                    )}
                    {check.error_count === 0 && check.warning_count === 0 && (
                      <Text color="green.500">No issues</Text>
                    )}
                  </HStack>
                </Box>
              ))}
            </HStack>
          </Box>
        )}

        {/* Artifact Summary */}
        <Box>
          <Heading size="md" mb={3}>Artifact Summary</Heading>
          <HStack gap={4} fontSize="sm">
            <Badge colorPalette="green">{run.artifact_summary.available} available</Badge>
            {run.artifact_summary.missing > 0 && (
              <Badge colorPalette="orange">{run.artifact_summary.missing} missing</Badge>
            )}
            {run.artifact_summary.failed > 0 && (
              <Badge colorPalette="red">{run.artifact_summary.failed} failed</Badge>
            )}
            {run.artifact_summary.skipped > 0 && (
              <Badge colorPalette="gray">{run.artifact_summary.skipped} skipped</Badge>
            )}
          </HStack>
        </Box>

        {/* Artifacts Table */}
        {artifacts.length > 0 && (
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
                {artifacts.map((artifact, idx) => (
                  <Table.Row key={artifact.artifact_id ?? idx}>
                    <Table.Cell>
                      <Text fontSize="sm">{artifact.type}</Text>
                    </Table.Cell>
                    <Table.Cell>
                      <Badge colorPalette={artifactStatusColor(artifact.status)}>
                        {artifact.status}
                      </Badge>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize="sm" fontFamily="mono">
                        {artifact.filename ?? artifact.status_reason ?? "—"}
                      </Text>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize="sm" color="gray.600">
                        {artifact.size_bytes
                          ? `${(artifact.size_bytes / 1024).toFixed(1)} KB`
                          : "—"}
                      </Text>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Root>
          </Box>
        )}

        {/* Viewers */}
        {Object.keys(viewers).length > 0 && (
          <Box>
            <Heading size="md" mb={3}>Viewers</Heading>
            <VStack align="stretch" gap={3}>
              {Object.entries(viewers).map(([name, viewer]) => (
                <Box
                  key={name}
                  borderWidth="1px"
                  borderRadius="md"
                  p={4}
                  bg="white"
                >
                  <HStack justify="space-between" mb={2}>
                    <Text fontWeight="medium" textTransform="capitalize">
                      {name.replace(/_/g, " ")}
                    </Text>
                    <Badge colorPalette={viewerStatusColor(viewer.status)}>
                      {viewer.status}
                    </Badge>
                  </HStack>
                  {viewer.status === "available" && viewer.primary && (
                    <a
                      href={viewer.primary.url}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      <Text color="blue.600" fontSize="sm" _hover={{ textDecoration: "underline" }}>
                        Open {viewer.primary.artifact_type ?? name}
                      </Text>
                    </a>
                  )}
                  {viewer.status === "available" && viewer.iframe_url && (
                    <a
                      href={viewer.iframe_url}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      <Text color="blue.600" fontSize="sm" _hover={{ textDecoration: "underline" }}>
                        Open {name}
                      </Text>
                    </a>
                  )}
                  {viewer.downloads && viewer.downloads.length > 0 && (
                    <HStack gap={2} mt={1}>
                      {viewer.downloads
                        .filter((d) => d.url)
                        .map((d) => (
                          <a
                            key={d.artifact_id ?? d.artifact_type}
                            href={d.url}
                            target="_blank"
                            rel="noopener noreferrer"
                          >
                            <Text color="blue.600" fontSize="sm" _hover={{ textDecoration: "underline" }}>
                              Download {d.artifact_type}
                            </Text>
                          </a>
                        ))}
                    </HStack>
                  )}
                  {(viewer.status === "missing" || viewer.status === "failed") && (
                    <Text fontSize="sm" color="gray.500">
                      {viewer.status === "missing"
                        ? "Required sources are not available."
                        : "Failed to generate viewer sources."}
                    </Text>
                  )}
                </Box>
              ))}
            </VStack>
          </Box>
        )}
      </VStack>
    </Box>
  )
}
