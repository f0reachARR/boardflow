import { Box, Heading, Table, Text, Badge, VStack, HStack } from "@chakra-ui/react"
import Link from "next/link"
import { notFound } from "next/navigation"
import { createServerClient } from "@/lib/api/server"

function stateColor(state: string): string {
  switch (state) {
    case "completed":
      return "green"
    case "failed":
      return "red"
    case "timed_out":
      return "orange"
    case "processing":
      return "blue"
    case "detected":
      return "gray"
    default:
      return "gray"
  }
}

interface Props {
  params: Promise<{ repositoryId: string }>
}

export default async function RepositoryDetailPage({ params }: Props) {
  const { repositoryId } = await params
  const client = await createServerClient()

  const [repoRes, projectsRes] = await Promise.all([
    client.GET("/api/v1/repositories/{github_repository_id}", {
      params: { path: { github_repository_id: repositoryId } },
    }),
    client.GET("/api/v1/repositories/{github_repository_id}/board-projects", {
      params: { path: { github_repository_id: repositoryId }, query: { limit: 50 } },
    }),
  ])

  if (repoRes.error) {
    notFound()
  }

  const repo = repoRes.data!
  const projects = projectsRes.data?.items ?? []

  return (
    <Box>
      <VStack align="stretch" gap={6}>
        <Box>
          <HStack gap={2} mb={1}>
            <Heading size="lg">{repo.owner}/{repo.name}</Heading>
          </HStack>
          <HStack gap={4} fontSize="sm" color="gray.600">
            <Text>{repo.board_project_count} projects</Text>
            <Text>Created {new Date(repo.created_at).toLocaleDateString()}</Text>
            {repo.html_url && (
              <a
                href={repo.html_url}
                target="_blank"
                rel="noopener noreferrer"
              >
                <Text color="blue.500" _hover={{ textDecoration: "underline" }}>
                  View on GitHub
                </Text>
              </a>
            )}
          </HStack>
        </Box>

        <Box>
          <Heading size="md" mb={4}>Board Projects</Heading>

          {projects.length === 0 ? (
            <Text color="gray.500">No board projects found.</Text>
          ) : (
            <Table.Root size="sm" variant="outline">
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
                      <Link href={`/repositories/${repositoryId}/boards/${project.board_project_id}`}>
                        <Text color="blue.600" fontWeight="medium" _hover={{ textDecoration: "underline" }}>
                          {project.display_name}
                        </Text>
                      </Link>
                    </Table.Cell>
                    <Table.Cell>
                      <Badge colorPalette={stateColor(project.state)}>
                        {project.state}
                      </Badge>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize="sm" color="gray.600" fontFamily="mono">
                        {project.project_path}
                      </Text>
                    </Table.Cell>
                    <Table.Cell>
                      <Text fontSize="sm" color="gray.600">
                        {new Date(project.updated_at).toLocaleDateString()}
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
  )
}
