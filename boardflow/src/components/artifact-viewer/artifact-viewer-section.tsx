"use client"

import { useEffect, useRef, useState, useCallback } from "react"
import { Box, Heading, VStack, HStack, Badge, Text } from "@chakra-ui/react"
import type { ViewerEntry, ViewerSourcesResponse } from "@/lib/api/schema"
import { PdfViewer } from "./pdf-viewer"
import { SvgViewer } from "./svg-viewer"
import { IbomViewer } from "./ibom-viewer"
import { DownloadList } from "./download-list"
import { ViewerStatusMessage } from "./viewer-status-message"

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

function viewerDisplayName(name: string): string {
  return name.replace(/_/g, " ")
}

interface ArtifactViewerSectionProps {
  viewers: Record<string, ViewerEntry>
  expiresAt?: string
  boardRunId: string
}

export function ArtifactViewerSection({
  viewers: initialViewers,
  expiresAt: initialExpiresAt,
  boardRunId,
}: ArtifactViewerSectionProps) {
  const [viewers, setViewers] = useState(initialViewers)
  const [expiresAt, setExpiresAt] = useState(initialExpiresAt)
  const [refreshError, setRefreshError] = useState(false)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const refreshViewerSources = useCallback(async () => {
    try {
      const res = await fetch(`/api/viewer-sources/${encodeURIComponent(boardRunId)}`)
      if (!res.ok) {
        setRefreshError(true)
        return
      }
      const data: ViewerSourcesResponse = await res.json()
      setViewers(data.viewers)
      setExpiresAt(data.expires_at)
      setRefreshError(false)
    } catch {
      setRefreshError(true)
    }
  }, [boardRunId])

  useEffect(() => {
    if (!expiresAt) return

    const expiresMs = new Date(expiresAt).getTime()
    const refreshAt = expiresMs - 5 * 60 * 1000 // 5 minutes before expiry
    const delay = refreshAt - Date.now()

    if (delay <= 0) {
      // Already past refresh time, refresh immediately
      refreshViewerSources()
      return
    }

    timerRef.current = setTimeout(() => {
      refreshViewerSources()
    }, delay)

    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current)
      }
    }
  }, [expiresAt, refreshViewerSources])

  if (Object.keys(viewers).length === 0) return null

  const hasPartial = Object.values(viewers).some((v) => v.status === "partial")

  return (
    <Box>
      <Heading size="md" mb={3}>
        Viewers
      </Heading>
      {refreshError && (
        <Box
          mb={4}
          p={3}
          borderWidth="1px"
          borderRadius="md"
          borderColor="red.200"
          bg="red.50"
        >
          <Text fontSize="sm" color="red.700" mb={2}>
            Viewer URLs have expired. Please reload the page.
          </Text>
          <button
            onClick={() => window.location.reload()}
            style={{
              padding: "4px 12px",
              fontSize: "0.875rem",
              borderRadius: "4px",
              border: "1px solid",
              borderColor: "var(--chakra-colors-red-300, #fc8181)",
              background: "white",
              cursor: "pointer",
            }}
          >
            Reload
          </button>
        </Box>
      )}
      {hasPartial && (
        <Text fontSize="sm" color="yellow.700" mb={3}>
          Some sources are unavailable. Showing limited preview.
        </Text>
      )}
      <VStack align="stretch" gap={4}>
        {Object.entries(viewers).map(([name, viewer]) => {
          if (name === "kicanvas") {
            return (
              <Box
                key={name}
                borderWidth="1px"
                borderRadius="md"
                p={4}
                bg="gray.50"
              >
                <HStack justify="space-between" mb={3}>
                  <Text fontWeight="medium" textTransform="capitalize">
                    KiCanvas
                  </Text>
                  <Badge colorPalette="gray">coming soon</Badge>
                </HStack>
                <Text fontSize="sm" color="gray.500">
                  KiCanvas interactive viewer will be available in a future update.
                </Text>
              </Box>
            )
          }

          return (
            <Box
              key={name}
              borderWidth="1px"
              borderRadius="md"
              p={4}
              bg="white"
            >
              <HStack justify="space-between" mb={3}>
                <Text fontWeight="medium" textTransform="capitalize">
                  {viewerDisplayName(name)}
                </Text>
                <Badge colorPalette={viewerStatusColor(viewer.status)}>
                  {viewer.status}
                </Badge>
              </HStack>
              {renderViewer(name, viewer)}
            </Box>
          )
        })}
      </VStack>
    </Box>
  )
}

function renderViewer(name: string, viewer: ViewerEntry) {
  const displayName = viewerDisplayName(name)

  if (viewer.status === "missing" || viewer.status === "failed" || viewer.status === "skipped") {
    return <ViewerStatusMessage status={viewer.status} viewerName={displayName} />
  }

  switch (name) {
    case "schematic":
      return (
        <>
          {viewer.primary && <PdfViewer primary={viewer.primary} />}
          {!viewer.primary && viewer.downloads && viewer.downloads.length > 0 && (
            <DownloadList downloads={viewer.downloads} title="Schematic Downloads" />
          )}
          {!viewer.primary && !viewer.downloads?.length && (
            <ViewerStatusMessage status="missing" viewerName={displayName} />
          )}
        </>
      )

    case "pcb_preview":
      return (
        <>
          {viewer.sources && viewer.sources.length > 0 && (
            <SvgViewer sources={viewer.sources} />
          )}
          {(!viewer.sources || viewer.sources.length === 0) && (
            <ViewerStatusMessage status="missing" viewerName={displayName} />
          )}
        </>
      )

    case "ibom":
      return (
        <>
          {viewer.iframe_url && <IbomViewer iframeUrl={viewer.iframe_url} />}
          {!viewer.iframe_url && (
            <ViewerStatusMessage status="missing" viewerName={displayName} />
          )}
        </>
      )

    default:
      // Generic download-based viewers (bom, fabrication, etc.)
      return (
        <>
          {viewer.downloads && viewer.downloads.length > 0 && (
            <DownloadList downloads={viewer.downloads} title={`${displayName} Downloads`} />
          )}
          {(!viewer.downloads || viewer.downloads.length === 0) && viewer.primary && (
            <a href={viewer.primary.url} target="_blank" rel="noopener noreferrer">
              <Text color="blue.600" fontSize="sm" _hover={{ textDecoration: "underline" }}>
                Open {viewer.primary.artifact_type ?? name}
              </Text>
            </a>
          )}
          {!viewer.downloads?.length && !viewer.primary && (
            <ViewerStatusMessage status="missing" viewerName={displayName} />
          )}
        </>
      )
  }
}
