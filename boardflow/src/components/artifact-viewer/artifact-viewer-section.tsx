"use client"

import { useEffect, useRef, useState, useCallback } from "react"
import { Box, Heading, Badge, Tabs, Text } from "@chakra-ui/react"
import type { ViewerEntry, ViewerSourcesResponse, ViewerSource } from "@/lib/api/schema-types"
import { PdfViewer } from "./pdf-viewer"
import { SvgViewer } from "./svg-viewer"
import { IbomViewer } from "./ibom-viewer"
import { KiCanvasViewer } from "./kicanvas-viewer"
import { DownloadList } from "./download-list"
import { ViewerStatusMessage } from "./viewer-status-message"

/** Ordered tab definitions */
const TAB_DEFINITIONS: { key: string; label: string }[] = [
  { key: "schematic", label: "Schematic" },
  { key: "pcb_preview", label: "PCB" },
  { key: "ibom", label: "iBOM" },
  { key: "bom", label: "BOM" },
  { key: "fabrication", label: "Fabrication" },
]

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

  // Build visible tabs from definitions, filtering out "skipped" viewers
  const visibleTabs = TAB_DEFINITIONS.filter((def) => {
    const viewer = viewers[def.key]
    if (!viewer) return false
    if (viewer.status === "skipped") return false
    // schematic / pcb_preview は kicanvas が available なら表示
    if ((def.key === "schematic" || def.key === "pcb_preview") &&
        (viewer.status === "missing" || viewer.status === "failed")) {
      const kicanvasViewer = viewers["kicanvas"]
      if (kicanvasViewer?.status === "available" && kicanvasViewer.sources?.length) {
        const relevantKind = def.key === "schematic" ? "schematic" : "board"
        return kicanvasViewer.sources.some((s: ViewerSource) => s.kind === relevantKind)
      }
    }
    return true
  })

  if (visibleTabs.length === 0) {
    return (
      <Box>
        <Heading size="md" mb={3}>
          Viewers
        </Heading>
        <Text fontSize="sm" color="gray.500">
          No viewers available for this run.
        </Text>
      </Box>
    )
  }

  const hasPartial = Object.values(viewers).some((v) => v.status === "partial")

  // Default tab: first available or partial viewer
  const defaultTab =
    visibleTabs.find((t) => {
      const v = viewers[t.key]
      return v && (v.status === "available" || v.status === "partial")
    })?.key ?? visibleTabs[0].key

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
      <Tabs.Root defaultValue={defaultTab} lazyMount>
        <Tabs.List>
          {visibleTabs.map((tab) => {
            const viewer = viewers[tab.key]!
            const isUnavailable =
              viewer.status === "missing" || viewer.status === "failed"
            return (
              <Tabs.Trigger
                key={tab.key}
                value={tab.key}
              >
                {tab.label}
                {isUnavailable && (
                  <Badge ml={1} size="xs" colorPalette="red">
                    {viewer.status}
                  </Badge>
                )}
              </Tabs.Trigger>
            )
          })}
        </Tabs.List>
        {visibleTabs.map((tab) => (
          <Tabs.Content key={tab.key} value={tab.key}>
            <Box pt={4}>
              {renderViewerContent(tab.key, viewers[tab.key]!, viewers)}
            </Box>
          </Tabs.Content>
        ))}
      </Tabs.Root>
    </Box>
  )
}

function renderViewerContent(name: string, viewer: ViewerEntry, allViewers: Record<string, ViewerEntry>) {
  // schematic / pcb_preview は KiCanvas fallback があるため、早期 return しない
  if (name !== "schematic" && name !== "pcb_preview") {
    if (viewer.status === "missing" || viewer.status === "failed") {
      return <ViewerStatusMessage status={viewer.status} viewerName={name} />
    }
  }

  switch (name) {
    case "schematic": {
      const kicanvasViewer = allViewers["kicanvas"]
      const kicanvasSchSources = kicanvasViewer?.status === "available"
        ? kicanvasViewer.sources?.filter((s: ViewerSource) => s.kind === "schematic" || s.kind === "project") ?? []
        : []
      const hasKicanvas = kicanvasSchSources.some((s: ViewerSource) => s.kind === "schematic")

      // static viewer が missing/failed でも KiCanvas があれば表示
      if (!hasKicanvas && (viewer.status === "missing" || viewer.status === "failed")) {
        return <ViewerStatusMessage status={viewer.status} viewerName="schematic" />
      }

      return (
        <>
          {hasKicanvas && <KiCanvasViewer sources={kicanvasSchSources} />}
          {viewer.primary && <PdfViewer primary={viewer.primary} />}
          {!hasKicanvas && !viewer.primary && viewer.downloads && viewer.downloads.length > 0 && (
            <DownloadList downloads={viewer.downloads} title="Schematic Downloads" />
          )}
        </>
      )
    }

    case "pcb_preview": {
      const kicanvasViewer = allViewers["kicanvas"]
      const kicanvasPcbSources = kicanvasViewer?.status === "available"
        ? kicanvasViewer.sources?.filter((s: ViewerSource) => s.kind === "board" || s.kind === "project") ?? []
        : []
      const hasKicanvas = kicanvasPcbSources.some((s: ViewerSource) => s.kind === "board")

      // static viewer が missing/failed でも KiCanvas があれば表示
      if (!hasKicanvas && (viewer.status === "missing" || viewer.status === "failed")) {
        return <ViewerStatusMessage status={viewer.status} viewerName="pcb_preview" />
      }

      return (
        <>
          {hasKicanvas && <KiCanvasViewer sources={kicanvasPcbSources} />}
          {viewer.sources && viewer.sources.length > 0 && (
            <SvgViewer sources={viewer.sources} />
          )}
        </>
      )
    }

    case "ibom":
      return (
        <>
          {viewer.iframe_url && <IbomViewer iframeUrl={viewer.iframe_url} />}
          {!viewer.iframe_url && (
            <ViewerStatusMessage status="missing" viewerName="ibom" />
          )}
        </>
      )

    default:
      // Generic download-based viewers (bom, fabrication, etc.)
      return (
        <>
          {viewer.downloads && viewer.downloads.length > 0 && (
            <DownloadList downloads={viewer.downloads} title={`${name} Downloads`} />
          )}
          {(!viewer.downloads || viewer.downloads.length === 0) && viewer.primary && (
            <a href={viewer.primary.url ?? undefined} target="_blank" rel="noopener noreferrer">
              <Text color="blue.600" fontSize="sm" _hover={{ textDecoration: "underline" }}>
                Open {viewer.primary.artifact_type ?? name}
              </Text>
            </a>
          )}
          {!viewer.downloads?.length && !viewer.primary && (
            <ViewerStatusMessage status="missing" viewerName={name} />
          )}
        </>
      )
  }
}
