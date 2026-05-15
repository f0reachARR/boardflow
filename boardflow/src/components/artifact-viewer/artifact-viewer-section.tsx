'use client';

import { Badge, Box, Heading, Tabs, Text } from '@chakra-ui/react';
import type { ViewerEntry } from '@/lib/api/schema-types';
import { useViewerSources } from './use-viewer-sources';
import { ViewerContent } from './viewer-content';
import { getDefaultViewerTab, getVisibleViewerTabs } from './viewer-selection';

interface ArtifactViewerSectionProps {
  viewers: Record<string, ViewerEntry>;
  expiresAt?: string;
  boardRunId: string;
}

export function ArtifactViewerSection({
  viewers: initialViewers,
  expiresAt: initialExpiresAt,
  boardRunId,
}: ArtifactViewerSectionProps) {
  const { viewers, isRefreshError: refreshError } = useViewerSources(
    initialViewers,
    initialExpiresAt,
    boardRunId,
  );

  // Build visible tabs from definitions, filtering out "skipped" viewers
  const visibleTabs = getVisibleViewerTabs(viewers);

  if (visibleTabs.length === 0) {
    return (
      <Box>
        <Heading size='md' mb={3}>
          Viewers
        </Heading>
        <Text fontSize='sm' color='gray.500'>
          No viewers available for this run.
        </Text>
      </Box>
    );
  }

  const hasPartial = Object.values(viewers).some((v) => v.status === 'partial');

  // Default tab: first available or partial viewer
  const defaultTab = getDefaultViewerTab(visibleTabs, viewers);

  return (
    <Box>
      <Heading size='md' mb={3}>
        Viewers
      </Heading>
      {refreshError && (
        <Box mb={4} p={3} borderWidth='1px' borderRadius='md' borderColor='red.200' bg='red.50'>
          <Text fontSize='sm' color='red.700' mb={2}>
            Viewer URLs have expired. Please reload the page.
          </Text>
          <button
            type='button'
            onClick={() => window.location.reload()}
            style={{
              padding: '4px 12px',
              fontSize: '0.875rem',
              borderRadius: '4px',
              border: '1px solid',
              borderColor: 'var(--chakra-colors-red-300, #fc8181)',
              background: 'white',
              cursor: 'pointer',
            }}
          >
            Reload
          </button>
        </Box>
      )}
      {hasPartial && (
        <Text fontSize='sm' color='yellow.700' mb={3}>
          Some sources are unavailable. Showing limited preview.
        </Text>
      )}
      <Tabs.Root defaultValue={defaultTab} lazyMount>
        <Tabs.List>
          {visibleTabs.map((tab) => {
            const viewer = viewers[tab.key];
            const isUnavailable = viewer.status === 'missing' || viewer.status === 'failed';
            return (
              <Tabs.Trigger key={tab.key} value={tab.key}>
                {tab.label}
                {isUnavailable && (
                  <Badge ml={1} size='xs' colorPalette='red'>
                    {viewer.status}
                  </Badge>
                )}
              </Tabs.Trigger>
            );
          })}
        </Tabs.List>
        {visibleTabs.map((tab) => (
          <Tabs.Content key={tab.key} value={tab.key}>
            <Box pt={4}>
              <ViewerContent name={tab.key} viewer={viewers[tab.key]} allViewers={viewers} />
            </Box>
          </Tabs.Content>
        ))}
      </Tabs.Root>
    </Box>
  );
}
