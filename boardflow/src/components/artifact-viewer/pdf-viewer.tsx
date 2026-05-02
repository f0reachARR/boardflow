"use client"

import { Box, HStack, Text } from "@chakra-ui/react"
import { Download, FileText } from "lucide-react"
import type { ViewerSource } from "@/lib/api/schema"

interface PdfViewerProps {
  primary: ViewerSource
}

export function PdfViewer({ primary }: PdfViewerProps) {
  return (
    <Box>
      <HStack gap={2} mb={2}>
        <FileText size={16} />
        <Text fontSize="sm" fontWeight="medium">
          Schematic PDF
        </Text>
        <a href={primary.url} target="_blank" rel="noopener noreferrer">
          <HStack gap={1} color="blue.600" _hover={{ textDecoration: "underline" }}>
            <Download size={14} />
            <Text fontSize="sm">Download</Text>
          </HStack>
        </a>
      </HStack>
      <Box borderWidth="1px" borderRadius="md" overflow="hidden">
        <iframe
          src={primary.url}
          width="100%"
          height="600px"
          title="Schematic PDF Viewer"
          style={{ display: "block" }}
        />
      </Box>
    </Box>
  )
}
