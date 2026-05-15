import type { ViewerEntry, ViewerSource } from '@/lib/api/schema-types';

export type TabDefinition = { key: string; label: string };

/** Ordered tab definitions */
export const TAB_DEFINITIONS: TabDefinition[] = [
  { key: 'schematic', label: 'Schematic' },
  { key: 'pcb_preview', label: 'PCB' },
  { key: 'ibom', label: 'iBOM' },
  { key: 'bom', label: 'BOM' },
  { key: 'fabrication', label: 'Fabrication' },
];

/**
 * Get KiCanvas sources for a given kind.
 * - schematic: filters sources with kind 'schematic' or 'project'
 * - board: filters sources with kind 'board'
 */
export function getKicanvasSources(
  allViewers: Record<string, ViewerEntry>,
  kind: 'schematic' | 'board',
): ViewerSource[] {
  const kicanvasViewer = allViewers.kicanvas;
  if (kicanvasViewer?.status === 'missing') return [];
  if (!kicanvasViewer?.sources?.length) return [];

  if (kind === 'schematic') {
    return kicanvasViewer.sources.filter((s) => s.kind === 'schematic' || s.kind === 'project');
  }
  return kicanvasViewer.sources.filter((s) => s.kind === 'board');
}

/**
 * Whether a KiCanvas fallback is available for the given kind.
 * - schematic: true if sources contain kind === 'schematic'
 * - board: true if sources contain kind === 'board'
 */
export function canUseKicanvasFallback(
  allViewers: Record<string, ViewerEntry>,
  kind: 'schematic' | 'board',
): boolean {
  const sources = getKicanvasSources(allViewers, kind);
  if (kind === 'schematic') {
    return sources.some((s) => s.kind === 'schematic');
  }
  return sources.some((s) => s.kind === 'board');
}

/** Build visible tabs, filtering out "skipped" viewers and applying KiCanvas fallback. */
export function getVisibleViewerTabs(viewers: Record<string, ViewerEntry>): TabDefinition[] {
  return TAB_DEFINITIONS.filter((def) => {
    const viewer = viewers[def.key];
    if (!viewer) return false;
    if (viewer.status === 'skipped') return false;
    // schematic / pcb_preview は kicanvas が available なら表示
    if (
      (def.key === 'schematic' || def.key === 'pcb_preview') &&
      (viewer.status === 'missing' || viewer.status === 'failed')
    ) {
      const kicanvasViewer = viewers.kicanvas;
      if (kicanvasViewer?.status === 'available' && kicanvasViewer.sources?.length) {
        const relevantKind = def.key === 'schematic' ? 'schematic' : 'board';
        return kicanvasViewer.sources.some((s) => s.kind === relevantKind);
      }
    }
    return true;
  });
}

/** Pick the default tab: first available/partial viewer, or the first visible tab. */
export function getDefaultViewerTab(
  visibleTabs: TabDefinition[],
  viewers: Record<string, ViewerEntry>,
): string {
  return (
    visibleTabs.find((t) => {
      const v = viewers[t.key];
      return v && (v.status === 'available' || v.status === 'partial');
    })?.key ?? visibleTabs[0].key
  );
}
