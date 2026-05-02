import { Box, Heading, Text, VStack, HStack, Badge, Table } from "@chakra-ui/react"
import Link from "next/link"
import { notFound } from "next/navigation"
import { createServerClient } from "@/lib/api/server"
import type { Artifact, ViewerEntry, DiffResponse } from "@/lib/api/schema"
import { Breadcrumb } from "@/components/ui/breadcrumb"
import { ArtifactViewerSection } from "@/components/artifact-viewer/artifact-viewer-section"

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

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>
}

export default async function RunDetailPage({ params }: Props) {
  const { repositoryId, boardProjectId, boardRunId } = await params
  const client = await createServerClient()

  const [runRes, artifactsRes, viewerRes, projectRes, diffRes] = await Promise.all([
    client.GET("/api/v1/board-runs/{board_run_id}", {
      params: { path: { board_run_id: boardRunId } },
    }),
    client.GET("/api/v1/board-runs/{board_run_id}/artifacts", {
      params: { path: { board_run_id: boardRunId } },
    }),
    client.GET("/api/v1/board-runs/{board_run_id}/viewer-sources", {
      params: { path: { board_run_id: boardRunId } },
    }),
    client.GET("/api/v1/board-projects/{board_project_id}", {
      params: { path: { board_project_id: boardProjectId } },
    }),
    client.GET("/api/v1/board-runs/{board_run_id}/diff", {
      params: { path: { board_run_id: boardRunId } },
    }),
  ])

  if (runRes.error) {
    notFound()
  }

  const run = runRes.data!
  const artifacts: Artifact[] = artifactsRes.data?.items ?? []
  const viewers: Record<string, ViewerEntry> = viewerRes.data?.viewers ?? {}
  const project = projectRes.data
  // Only hide diff section on 404 (diff not yet created); other errors surface as explicit messages
  const diff: DiffResponse | null = diffRes.error
    ? null
    : diffRes.data!
  const diffError = diffRes.error && diffRes.error.error?.code !== "not_found"
    ? diffRes.error.error?.message ?? "Failed to load diff data."
    : null

  return (
    <Box>
      {project && (
        <Breadcrumb
          items={[
            { label: "Repositories", href: "/repositories" },
            { label: `${project.repository.owner}/${project.repository.name}`, href: `/repositories/${repositoryId}` },
            { label: project.display_name, href: `/repositories/${repositoryId}/boards/${boardProjectId}` },
            { label: "Runs", href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs` },
            { label: boardRunId.slice(0, 8) },
          ]}
        />
      )}
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
                    {check.notice_count > 0 && (
                      <Text color="gray.500">{check.notice_count} notices</Text>
                    )}
                    {check.error_count === 0 && check.warning_count === 0 && check.notice_count === 0 && (
                      <Text color="green.500">No issues</Text>
                    )}
                  </HStack>
                  {(check.error_count + check.warning_count + check.notice_count) > 0 && (
                    <Link href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${check.kind}`}>
                      <Text color="blue.600" fontSize="sm" mt={2} _hover={{ textDecoration: "underline" }}>
                        View {check.error_count + check.warning_count + check.notice_count} findings
                      </Text>
                    </Link>
                  )}
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

        {/* Changes from Baseline */}
        {diffError && (
          <Box>
            <Heading size="md" mb={3}>Changes from Baseline</Heading>
            <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
              <Text fontSize="sm" color="red.500">{diffError}</Text>
            </Box>
          </Box>
        )}
        {diff && (
          <Box>
            <Heading size="md" mb={3}>Changes from Baseline</Heading>
            <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
              {diff.status === "ready" && diff.summary && (
                <VStack align="stretch" gap={2}>
                  {diff.base_board_run_id && (
                    <Text fontSize="sm" color="gray.600">
                      Compared to run:{" "}
                      <Link href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${diff.base_board_run_id}`}>
                        <Text as="span" color="blue.600" _hover={{ textDecoration: "underline" }}>
                          {diff.base_board_run_id.slice(0, 8)}
                        </Text>
                      </Link>
                    </Text>
                  )}
                  <HStack gap={4} fontSize="sm">
                    <Text>
                      Files: +{diff.summary.file_changes.added} -{diff.summary.file_changes.removed} ~{diff.summary.file_changes.changed} ({diff.summary.file_changes.unchanged} unchanged)
                    </Text>
                  </HStack>
                  <HStack gap={4} fontSize="sm">
                    <Text>
                      BOM: +{diff.summary.bom_changes.added} -{diff.summary.bom_changes.removed} ~{diff.summary.bom_changes.changed}
                    </Text>
                  </HStack>
                  {Object.keys(diff.summary.checks).length > 0 && (
                    <HStack gap={4} fontSize="sm" flexWrap="wrap">
                      <Text>Checks:</Text>
                      {Object.entries(diff.summary.checks).map(([kind, c]) => (
                        <Text key={kind}>
                          {kind.toUpperCase()} {c.status_change} ({c.error_delta >= 0 ? "+" : ""}{c.error_delta}E, {c.warning_delta >= 0 ? "+" : ""}{c.warning_delta}W)
                        </Text>
                      ))}
                    </HStack>
                  )}
                  <HStack gap={4} fontSize="sm">
                    <Text>
                      Artifacts: +{diff.summary.artifacts.added} -{diff.summary.artifacts.removed} ~{diff.summary.artifacts.changed}
                    </Text>
                  </HStack>
                </VStack>
              )}
              {diff.status === "no_baseline" && (
                <Text fontSize="sm" color="gray.500">
                  This is the first run. No baseline for comparison.
                </Text>
              )}
              {diff.status === "failed" && (
                <Text fontSize="sm" color="red.500">
                  {diff.error_message ?? "Diff computation failed."}
                </Text>
              )}
              {diff.status === "unavailable" && (
                <Text fontSize="sm" color="gray.500">
                  Diff data is not available.
                </Text>
              )}
            </Box>
          </Box>
        )}

        {/* Viewers */}
        <ArtifactViewerSection
          viewers={viewers}
          expiresAt={viewerRes.data?.expires_at}
          boardRunId={boardRunId}
        />
      </VStack>
    </Box>
  )
}
