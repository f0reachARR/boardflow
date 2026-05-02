import { Box, Heading, Text, VStack, HStack, Badge, Table } from "@chakra-ui/react"
import Link from "next/link"
import { notFound } from "next/navigation"
import { createServerClient } from "@/lib/api/server"
import type { DiffResponse, DiffSummary } from "@/lib/api/schema"
import { Breadcrumb } from "@/components/ui/breadcrumb"

function diffStatusColor(status: string): string {
  switch (status) {
    case "ready":
      return "green"
    case "no_baseline":
      return "gray"
    case "unavailable":
      return "orange"
    case "failed":
      return "red"
    default:
      return "gray"
  }
}

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string }>
}

export default async function DiffPage({ params }: Props) {
  const { repositoryId, boardProjectId, boardRunId } = await params
  const client = await createServerClient()

  const [diffRes, projectRes] = await Promise.all([
    client.GET("/api/v1/board-runs/{board_run_id}/diff", {
      params: { path: { board_run_id: boardRunId } },
    }),
    client.GET("/api/v1/board-projects/{board_project_id}", {
      params: { path: { board_project_id: boardProjectId } },
    }),
  ])

  if (diffRes.error) {
    notFound()
  }

  const diff: DiffResponse = diffRes.data!
  const project = projectRes.data

  return (
    <Box>
      {project && (
        <Breadcrumb
          items={[
            { label: "Repositories", href: "/repositories" },
            { label: `${project.repository.owner}/${project.repository.name}`, href: `/repositories/${repositoryId}` },
            { label: project.display_name, href: `/repositories/${repositoryId}/boards/${boardProjectId}` },
            { label: "Runs", href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs` },
            { label: boardRunId.slice(0, 8), href: `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}` },
            { label: "Diff" },
          ]}
        />
      )}

      <VStack align="stretch" gap={6}>
        {/* Header */}
        <Box>
          <HStack gap={3} mb={2}>
            <Heading size="lg">Diff</Heading>
            <Badge colorPalette={diffStatusColor(diff.status)} size="lg">
              {diff.status}
            </Badge>
          </HStack>
          <HStack gap={4} fontSize="sm" color="gray.600">
            {diff.base_board_run_id && (
              <Text>
                Compared to:{" "}
                <Link href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${diff.base_board_run_id}`}>
                  <Text as="span" color="blue.600" _hover={{ textDecoration: "underline" }}>
                    {diff.base_board_run_id.slice(0, 8)}
                  </Text>
                </Link>
              </Text>
            )}
            <Text>
              Current:{" "}
              <Link href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}`}>
                <Text as="span" color="blue.600" _hover={{ textDecoration: "underline" }}>
                  {boardRunId.slice(0, 8)}
                </Text>
              </Link>
            </Text>
            <Text>Created {new Date(diff.created_at).toLocaleString()}</Text>
          </HStack>
        </Box>

        {/* Status-based content */}
        {diff.status === "no_baseline" && (
          <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
            <Text fontSize="sm" color="gray.500">
              This is the first completed run. No baseline available for comparison.
            </Text>
          </Box>
        )}

        {diff.status === "unavailable" && (
          <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
            <Text fontSize="sm" color="gray.500">
              Diff data is not available. The baseline or current run may be missing required data.
            </Text>
          </Box>
        )}

        {diff.status === "failed" && (
          <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
            <Text fontSize="sm" color="red.500">
              {diff.error_message ?? "Diff computation failed."}
            </Text>
          </Box>
        )}

        {diff.status === "ready" && diff.summary && (
          <>
            <FileChangesSection summary={diff.summary} metadata={diff.metadata} />
            <BomChangesSection summary={diff.summary} metadata={diff.metadata} />
            <ChecksSection summary={diff.summary} />
            <ArtifactChangesSection summary={diff.summary} />
          </>
        )}
      </VStack>
    </Box>
  )
}

function FileChangesSection({ summary, metadata }: { summary: DiffSummary; metadata: Record<string, unknown> | null }) {
  const { added, removed, changed, unchanged } = summary.file_changes
  const fileHashes = metadata?.file_hashes as { changed_files?: string[] } | undefined
  const changedFiles = fileHashes?.changed_files

  return (
    <Box>
      <Heading size="md" mb={3}>File Changes</Heading>
      <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
        <HStack gap={4} fontSize="sm" mb={changedFiles ? 3 : 0}>
          <Badge colorPalette="green">+{added} added</Badge>
          <Badge colorPalette="red">-{removed} removed</Badge>
          <Badge colorPalette="yellow">~{changed} changed</Badge>
          <Badge colorPalette="gray">{unchanged} unchanged</Badge>
        </HStack>
        {changedFiles && changedFiles.length > 0 && (
          <Box mt={2}>
            <Text fontSize="sm" fontWeight="bold" mb={1}>Changed files:</Text>
            <VStack align="stretch" gap={1}>
              {changedFiles.map((file) => (
                <Text key={file} fontSize="sm" fontFamily="mono" color="gray.700">
                  {file}
                </Text>
              ))}
            </VStack>
          </Box>
        )}
      </Box>
    </Box>
  )
}

function BomChangesSection({ summary, metadata }: { summary: DiffSummary; metadata: Record<string, unknown> | null }) {
  const { added, removed, changed } = summary.bom_changes
  const bomSummary = metadata?.bom_summary as { rows?: Array<Record<string, string>> } | undefined
  const rows = bomSummary?.rows

  return (
    <Box>
      <Heading size="md" mb={3}>BOM Changes</Heading>
      <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
        <HStack gap={4} fontSize="sm" mb={rows ? 3 : 0}>
          <Badge colorPalette="green">+{added} added</Badge>
          <Badge colorPalette="red">-{removed} removed</Badge>
          <Badge colorPalette="yellow">~{changed} changed</Badge>
        </HStack>
        {rows && rows.length > 0 && (
          <Table.Root size="sm" variant="outline" mt={2}>
            <Table.Header>
              <Table.Row>
                {Object.keys(rows[0]).map((key) => (
                  <Table.ColumnHeader key={key}>{key}</Table.ColumnHeader>
                ))}
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {rows.map((row, idx) => (
                <Table.Row key={idx}>
                  {Object.values(row).map((val, cidx) => (
                    <Table.Cell key={cidx}>
                      <Text fontSize="sm">{val}</Text>
                    </Table.Cell>
                  ))}
                </Table.Row>
              ))}
            </Table.Body>
          </Table.Root>
        )}
      </Box>
    </Box>
  )
}

function ChecksSection({ summary }: { summary: DiffSummary }) {
  const checks = Object.entries(summary.checks)
  if (checks.length === 0) return null

  return (
    <Box>
      <Heading size="md" mb={3}>ERC/DRC Checks</Heading>
      <HStack gap={4} flexWrap="wrap">
        {checks.map(([kind, check]) => (
          <Box
            key={kind}
            borderWidth="1px"
            borderRadius="md"
            p={4}
            bg="white"
            minW="200px"
          >
            <Text fontWeight="bold" textTransform="uppercase" mb={2}>
              {kind}
            </Text>
            <VStack align="stretch" gap={1} fontSize="sm">
              <Text>Status: {check.status_change}</Text>
              <HStack gap={3}>
                <Text color={check.error_delta > 0 ? "red.500" : check.error_delta < 0 ? "green.500" : "gray.500"}>
                  {check.error_delta >= 0 ? "+" : ""}{check.error_delta} errors
                </Text>
                <Text color={check.warning_delta > 0 ? "orange.500" : check.warning_delta < 0 ? "green.500" : "gray.500"}>
                  {check.warning_delta >= 0 ? "+" : ""}{check.warning_delta} warnings
                </Text>
              </HStack>
            </VStack>
          </Box>
        ))}
      </HStack>
    </Box>
  )
}

function ArtifactChangesSection({ summary }: { summary: DiffSummary }) {
  const { added, removed, changed } = summary.artifacts

  return (
    <Box>
      <Heading size="md" mb={3}>Artifact Changes</Heading>
      <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
        <HStack gap={4} fontSize="sm">
          <Badge colorPalette="green">+{added} added</Badge>
          <Badge colorPalette="red">-{removed} removed</Badge>
          <Badge colorPalette="yellow">~{changed} changed</Badge>
        </HStack>
      </Box>
    </Box>
  )
}
