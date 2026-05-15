'use client';

import { Text } from '@chakra-ui/react';
import type { ViewerEntry } from '@/lib/api/schema-types';
import { DownloadList } from './download-list';
import { IbomViewer } from './ibom-viewer';
import { KiCanvasViewer } from './kicanvas-viewer';
import { PdfViewer } from './pdf-viewer';
import { SvgViewer } from './svg-viewer';
import { canUseKicanvasFallback, getKicanvasSources } from './viewer-selection';
import { ViewerStatusMessage } from './viewer-status-message';

interface ViewerContentProps {
  name: string;
  viewer: ViewerEntry;
  allViewers: Record<string, ViewerEntry>;
}

export function ViewerContent({ name, viewer, allViewers }: ViewerContentProps) {
  // schematic / pcb_preview は KiCanvas fallback があるため、早期 return しない
  if (name !== 'schematic' && name !== 'pcb_preview') {
    if (viewer.status === 'missing' || viewer.status === 'failed') {
      return <ViewerStatusMessage status={viewer.status} viewerName={name} />;
    }
  }

  switch (name) {
    case 'schematic':
      return <SchematicContent viewer={viewer} allViewers={allViewers} />;
    case 'pcb_preview':
      return <PcbPreviewContent viewer={viewer} allViewers={allViewers} />;
    case 'ibom':
      return <IbomContent viewer={viewer} />;
    default:
      return <GenericDownloadContent name={name} viewer={viewer} />;
  }
}

function SchematicContent({
  viewer,
  allViewers,
}: {
  viewer: ViewerEntry;
  allViewers: Record<string, ViewerEntry>;
}) {
  const kicanvasSchSources = getKicanvasSources(allViewers, 'schematic');
  const hasKicanvas = canUseKicanvasFallback(allViewers, 'schematic');

  // static viewer が missing/failed でも KiCanvas があれば表示
  if (!hasKicanvas && (viewer.status === 'missing' || viewer.status === 'failed')) {
    return <ViewerStatusMessage status={viewer.status} viewerName='schematic' />;
  }

  return (
    <>
      {hasKicanvas && <KiCanvasViewer sources={kicanvasSchSources} />}
      {viewer.primary && <PdfViewer primary={viewer.primary} />}
      {!hasKicanvas && !viewer.primary && viewer.downloads && viewer.downloads.length > 0 && (
        <DownloadList downloads={viewer.downloads} title='Schematic Downloads' />
      )}
    </>
  );
}

function PcbPreviewContent({
  viewer,
  allViewers,
}: {
  viewer: ViewerEntry;
  allViewers: Record<string, ViewerEntry>;
}) {
  const kicanvasPcbSources = getKicanvasSources(allViewers, 'board');
  const hasKicanvas = canUseKicanvasFallback(allViewers, 'board');

  // static viewer が missing/failed でも KiCanvas があれば表示
  if (!hasKicanvas && (viewer.status === 'missing' || viewer.status === 'failed')) {
    return <ViewerStatusMessage status={viewer.status} viewerName='pcb_preview' />;
  }

  return (
    <>
      {hasKicanvas && <KiCanvasViewer sources={kicanvasPcbSources} />}
      {viewer.sources && viewer.sources.length > 0 && <SvgViewer sources={viewer.sources} />}
    </>
  );
}

function IbomContent({ viewer }: { viewer: ViewerEntry }) {
  return (
    <>
      {viewer.iframe_url && <IbomViewer iframeUrl={viewer.iframe_url} />}
      {!viewer.iframe_url && <ViewerStatusMessage status='missing' viewerName='ibom' />}
    </>
  );
}

function GenericDownloadContent({ name, viewer }: { name: string; viewer: ViewerEntry }) {
  return (
    <>
      {viewer.downloads && viewer.downloads.length > 0 && (
        <DownloadList downloads={viewer.downloads} title={`${name} Downloads`} />
      )}
      {(!viewer.downloads || viewer.downloads.length === 0) && viewer.primary && (
        <a href={viewer.primary.url ?? undefined} target='_blank' rel='noopener noreferrer'>
          <Text color='blue.600' fontSize='sm' _hover={{ textDecoration: 'underline' }}>
            Open {viewer.primary.artifact_type ?? name}
          </Text>
        </a>
      )}
      {!viewer.downloads?.length && !viewer.primary && (
        <ViewerStatusMessage status='missing' viewerName={name} />
      )}
    </>
  );
}
