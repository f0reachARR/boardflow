import { Box, Heading, Text, Table, Badge, HStack } from "@chakra-ui/react"
import Link from "next/link"
import { notFound } from "next/navigation"
import { createServerClient } from "@/lib/api/server"
import { Breadcrumb } from "@/components/ui/breadcrumb"

function severityColor(severity: string): string {
  switch (severity) {
    case "error":
      return "red"
    case "warning":
      return "orange"
    case "notice":
      return "gray"
    default:
      return "gray"
  }
}

function locationText(finding: { sheet_path: string | null; pcb_layer: string | null; pos_mm: { x: number; y: number } | null }): string {
  if (finding.sheet_path) {
    return `${finding.sheet_path} (schematic)`
  }
  if (finding.pcb_layer) {
    const pos = finding.pos_mm ? ` @ (${finding.pos_mm.x}, ${finding.pos_mm.y})` : ""
    return `${finding.pcb_layer}${pos}`
  }
  return "—"
}

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string; boardRunId: string; checkKind: string }>
  searchParams: Promise<{ severity?: string }>
}

const VALID_CHECK_KINDS = ["erc", "drc"]
const VALID_SEVERITIES = ["error", "warning", "notice"]

export default async function FindingsPage({ params, searchParams }: Props) {
  const { repositoryId, boardProjectId, boardRunId, checkKind } = await params
  const { severity } = await searchParams
  const client = await createServerClient()

  if (!VALID_CHECK_KINDS.includes(checkKind)) {
    return (
      <Box>
        <Heading size="lg" mb={4}>Invalid Check Kind</Heading>
        <Text color="red.500">
          &quot;{checkKind}&quot; is not a valid check kind. Supported values: {VALID_CHECK_KINDS.join(", ")}.
        </Text>
      </Box>
    )
  }

  if (severity && !VALID_SEVERITIES.includes(severity)) {
    return (
      <Box>
        <Heading size="lg" mb={4}>{checkKind.toUpperCase()} Findings</Heading>
        <Text color="red.500">
          &quot;{severity}&quot; is not a valid severity filter. Supported values: {VALID_SEVERITIES.join(", ")}.
        </Text>
      </Box>
    )
  }

  const validCheckKind = checkKind as "erc" | "drc"
  const validSeverityParam = severity as "error" | "warning" | "notice" | undefined

  const [findingsRes, projectRes] = await Promise.all([
    client.GET("/api/v1/board-runs/{board_run_id}/checks/{check_kind}/findings", {
      params: {
        path: { board_run_id: boardRunId, check_kind: validCheckKind },
        query: validSeverityParam ? { severity: validSeverityParam } : undefined,
      },
    }),
    client.GET("/api/v1/board-projects/{board_project_id}", {
      params: { path: { board_project_id: boardProjectId } },
    }),
  ])

  if (findingsRes.error) {
    const code = findingsRes.error.error?.code
    if (code === "not_found") {
      notFound()
    }
    return (
      <Box>
        <Heading size="lg" mb={4}>{checkKind.toUpperCase()} Findings</Heading>
        <Text color="red.500">Failed to load findings: {findingsRes.error.error?.message ?? "Unknown error"}</Text>
      </Box>
    )
  }

  const findings = findingsRes.data!.items
  const hasMore = findingsRes.data!.has_more
  const project = projectRes.data

  const basePath = `/repositories/${repositoryId}/boards/${boardProjectId}/runs/${boardRunId}/checks/${checkKind}`

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
            { label: `${checkKind.toUpperCase()} Findings` },
          ]}
        />
      )}
      <Heading size="lg" mb={4}>{checkKind.toUpperCase()} Findings</Heading>

      {/* Severity filter */}
      <HStack gap={2} mb={4}>
        <Link href={basePath}>
          <Badge colorPalette={!severity ? "blue" : "gray"} size="sm" cursor="pointer">
            All
          </Badge>
        </Link>
        <Link href={`${basePath}?severity=error`}>
          <Badge colorPalette={severity === "error" ? "red" : "gray"} size="sm" cursor="pointer">
            Errors
          </Badge>
        </Link>
        <Link href={`${basePath}?severity=warning`}>
          <Badge colorPalette={severity === "warning" ? "orange" : "gray"} size="sm" cursor="pointer">
            Warnings
          </Badge>
        </Link>
        <Link href={`${basePath}?severity=notice`}>
          <Badge colorPalette={severity === "notice" ? "gray" : "gray"} size="sm" cursor="pointer" variant={severity === "notice" ? "solid" : "outline"}>
            Notices
          </Badge>
        </Link>
      </HStack>

      {findings.length === 0 ? (
        <Text color="gray.500">No findings{severity ? ` with severity "${severity}"` : ""}.</Text>
      ) : (
        <>
          <Table.Root size="sm" variant="outline">
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
                  <Badge colorPalette={severityColor(finding.severity)} size="sm">
                    {finding.severity}
                  </Badge>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize="sm" fontFamily="mono">{finding.rule_code}</Text>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize="sm">{finding.title}</Text>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize="sm" color="gray.600">
                    {finding.message ?? "—"}
                  </Text>
                </Table.Cell>
                <Table.Cell>
                  <Text fontSize="sm" color="gray.600">
                    {locationText(finding)}
                  </Text>
                </Table.Cell>
              </Table.Row>
            ))}
          </Table.Body>
        </Table.Root>
          {hasMore && (
            <Text fontSize="sm" color="gray.500" mt={3}>
              More results available. Showing first {findings.length} findings.
            </Text>
          )}
        </>
      )}
    </Box>
  )
}
