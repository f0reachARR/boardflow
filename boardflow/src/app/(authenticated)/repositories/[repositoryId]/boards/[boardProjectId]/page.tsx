import { Box, Heading, Text, VStack, HStack, Badge } from "@chakra-ui/react"
import Link from "next/link"
import { notFound } from "next/navigation"
import { createServerClient } from "@/lib/api/server"
import { Breadcrumb } from "@/components/ui/breadcrumb"

interface Props {
  params: Promise<{ repositoryId: string; boardProjectId: string }>
}

export default async function BoardProjectDetailPage({ params }: Props) {
  const { repositoryId, boardProjectId } = await params
  const client = await createServerClient()

  const { data, error } = await client.GET("/api/v1/board-projects/{board_project_id}", {
    params: { path: { board_project_id: boardProjectId } },
  })

  if (error) {
    notFound()
  }

  const project = data!

  return (
    <Box>
      <Breadcrumb
        items={[
          { label: "Repositories", href: "/repositories" },
          { label: `${project.repository.owner}/${project.repository.name}`, href: `/repositories/${repositoryId}` },
          { label: project.display_name },
        ]}
      />
      <VStack align="stretch" gap={6}>
        <Box>
          <HStack gap={2} mb={1}>
            <Heading size="lg">{project.display_name}</Heading>
            <Badge colorPalette={project.state === "completed" ? "green" : "gray"}>
              {project.state}
            </Badge>
          </HStack>
          <Text fontSize="sm" color="gray.600" fontFamily="mono">
            {project.project_path}
          </Text>
        </Box>

        <Box borderWidth="1px" borderRadius="md" p={4} bg="white">
          <VStack align="stretch" gap={3}>
            <HStack justify="space-between">
              <Text fontWeight="medium">Repository</Text>
              <Link href={`/repositories/${repositoryId}`}>
                <Text color="blue.600" _hover={{ textDecoration: "underline" }}>
                  {project.repository.owner}/{project.repository.name}
                </Text>
              </Link>
            </HStack>
            <HStack justify="space-between">
              <Text fontWeight="medium">Project Directory</Text>
              <Text fontFamily="mono" fontSize="sm">{project.project_dir}</Text>
            </HStack>
            {project.issue_url && (
              <HStack justify="space-between">
                <Text fontWeight="medium">Issue</Text>
                <a
                  href={project.issue_url}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  <Text color="blue.600" _hover={{ textDecoration: "underline" }}>
                    #{project.issue_number}
                  </Text>
                </a>
              </HStack>
            )}
            {project.latest_completed_run_id && (
              <HStack justify="space-between">
                <Text fontWeight="medium">Latest Completed Run</Text>
                <Link href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs/${project.latest_completed_run_id}`}>
                  <Text color="blue.600" _hover={{ textDecoration: "underline" }}>
                    {project.latest_completed_run_id}
                  </Text>
                </Link>
              </HStack>
            )}
            <HStack justify="space-between">
              <Text fontWeight="medium">Created</Text>
              <Text fontSize="sm" color="gray.600">
                {new Date(project.created_at).toLocaleString()}
              </Text>
            </HStack>
          </VStack>
        </Box>

        <Box>
          <Link href={`/repositories/${repositoryId}/boards/${boardProjectId}/runs`}>
            <Box
              display="inline-flex"
              alignItems="center"
              px={4}
              py={2}
              borderRadius="md"
              bg="blue.600"
              color="white"
              fontWeight="medium"
              fontSize="sm"
              _hover={{ bg: "blue.700" }}
            >
              View All Runs
            </Box>
          </Link>
        </Box>
      </VStack>
    </Box>
  )
}
