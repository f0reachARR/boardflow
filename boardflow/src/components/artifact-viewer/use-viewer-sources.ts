'use client';

import { useQuery } from '@tanstack/react-query';
import type { ViewerEntry } from '@/lib/api/schema-types';

interface UseViewerSourcesResult {
  viewers: Record<string, ViewerEntry>;
  isRefreshError: boolean;
}

export function useViewerSources(
  initialViewers: Record<string, ViewerEntry>,
  initialExpiresAt: string | undefined,
  boardRunId: string,
): UseViewerSourcesResult {
  const { data, isError: isRefreshError } = useQuery({
    queryKey: ['viewer-sources', boardRunId],
    queryFn: async (): Promise<{
      viewers: Record<string, ViewerEntry>;
      expires_at?: string;
    }> => {
      const res = await fetch(`/api/viewer-sources/${encodeURIComponent(boardRunId)}`);
      if (!res.ok) throw new Error('Failed to refresh viewer sources');
      return res.json();
    },
    initialData: { viewers: initialViewers, expires_at: initialExpiresAt },
    refetchInterval: 4 * 60 * 1000,
    refetchIntervalInBackground: true,
  });

  return { viewers: data.viewers, isRefreshError };
}
