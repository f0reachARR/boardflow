"use client"

import { useEffect, useState } from "react"
import { Box, Text, Spinner } from "@chakra-ui/react"
import type { ViewerSource } from "@/lib/api/schema"

type LoadState = "loading" | "ready" | "error"

interface KiCanvasViewerProps {
  sources: ViewerSource[]
}

export function KiCanvasViewer({ sources }: KiCanvasViewerProps) {
  const [loadState, setLoadState] = useState<LoadState>("loading")

  useEffect(() => {
    let cancelled = false
    const timeout = setTimeout(() => {
      if (!cancelled && !customElements.get("kicanvas-embed")) {
        setLoadState("error")
      }
    }, 10000)

    if (customElements.get("kicanvas-embed")) {
      setLoadState("ready")
      clearTimeout(timeout)
      return
    }

    // @ts-expect-error dynamic vendor import bypasses webpack
    import(/* webpackIgnore: true */ "/vendor/kicanvas/kicanvas.js")
      .then(() => {
        return customElements.whenDefined("kicanvas-embed")
      })
      .then(() => {
        if (!cancelled) {
          setLoadState("ready")
          clearTimeout(timeout)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLoadState("error")
          clearTimeout(timeout)
        }
      })

    return () => {
      cancelled = true
      clearTimeout(timeout)
    }
  }, [])

  if (loadState === "error") {
    return (
      <Box p={4} bg="red.50" borderWidth="1px" borderRadius="md" borderColor="red.200">
        <Text fontWeight="medium" color="red.700">
          KiCanvas の読み込みに失敗しました
        </Text>
        <Text fontSize="sm" color="red.600" mt={1}>
          ブラウザが WebGL をサポートしていないか、スクリプトの読み込みに失敗しました。
        </Text>
      </Box>
    )
  }

  if (loadState === "loading") {
    return (
      <Box
        display="flex"
        alignItems="center"
        justifyContent="center"
        minH="500px"
        bg="gray.50"
        borderWidth="1px"
        borderRadius="md"
      >
        <Spinner size="lg" mr={3} />
        <Text color="gray.600">Loading KiCanvas...</Text>
      </Box>
    )
  }

  const kicanvasSources = sources
    .filter((s) => s.kind && s.url)
    .map((s) => ({
      type: s.kind as "project" | "schematic" | "board" | "worksheet",
      name: s.name ?? "",
      url: s.url,
    }))

  return (
    <Box minH="500px" borderWidth="1px" borderRadius="md" overflow="hidden">
      <kicanvas-embed
        controls="full"
        controlslist="nodownload"
        style={{ width: "100%", height: "600px", display: "block" }}
      >
        {kicanvasSources.map((source) => (
          <kicanvas-source
            key={`${source.type}:${source.name}`}
            src={source.url}
            type={source.type}
            name={source.name}
          />
        ))}
      </kicanvas-embed>
    </Box>
  )
}
