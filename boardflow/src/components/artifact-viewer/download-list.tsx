"use client"

import { Box, HStack, Text, VStack } from "@chakra-ui/react"
import { Download } from "lucide-react"
import type { ViewerDownload } from "@/lib/api/schema"

interface DownloadListProps {
  downloads: ViewerDownload[]
  title: string
}

export function DownloadList({ downloads, title }: DownloadListProps) {
  const available = downloads.filter((d) => d.url && d.status !== "missing" && d.status !== "failed")
  const unavailable = downloads.filter((d) => !d.url || d.status === "missing" || d.status === "failed")

  return (
    <Box>
      <Text fontSize="sm" fontWeight="medium" mb={2}>
        {title}
      </Text>
      <VStack align="stretch" gap={1}>
        {available.map((d) => (
          <a
            key={d.artifact_id ?? d.artifact_type}
            href={d.url!}
            target="_blank"
            rel="noopener noreferrer"
          >
            <HStack gap={2} color="blue.600" _hover={{ textDecoration: "underline" }}>
              <Download size={14} />
              <Text fontSize="sm">{d.artifact_type}</Text>
            </HStack>
          </a>
        ))}
        {unavailable.map((d) => (
          <HStack key={d.artifact_id ?? d.artifact_type} gap={2}>
            <Download size={14} color="gray" />
            <Text fontSize="sm" color="gray.500">
              {d.artifact_type} — {d.status_reason ?? d.status ?? "unavailable"}
            </Text>
          </HStack>
        ))}
      </VStack>
    </Box>
  )
}
