import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { RepositoriesList } from '@/components/repositories/repositories-list';
import { RepositoriesTableSkeleton } from '@/components/skeletons/repositories-table-skeleton';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { getQueryClient } from '@/lib/query-client';

export default async function RepositoriesPage() {
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  const options = $api.queryOptions('get', '/api/v1/repositories', {
    params: { query: { limit: 50 } },
  });

  // await しない → Streaming SSR: 結果が到着次第クライアントに反映
  queryClient.prefetchQuery({
    ...options,
    queryFn: async () => {
      const { data, error } = await serverClient.GET('/api/v1/repositories', {
        params: { query: { limit: 50 } },
      });
      if (error) throw new Error('Failed to fetch repositories');
      return data;
    },
  });

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<RepositoriesTableSkeleton />}>
        <RepositoriesList />
      </Suspense>
    </HydrationBoundary>
  );
}
