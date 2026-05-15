import { dehydrate, HydrationBoundary } from '@tanstack/react-query';
import { Suspense } from 'react';
import { RepositoriesList } from '@/components/repositories/repositories-list';
import { RepositoriesTableSkeleton } from '@/components/skeletons/repositories-table-skeleton';
import { $api } from '@/lib/api/react-query';
import { createServerClient } from '@/lib/api/server';
import { prefetchSecondary, withServerFetcher } from '@/lib/api/server-prefetch';
import { getQueryClient } from '@/lib/query-client';

export default async function RepositoriesPage() {
  const queryClient = getQueryClient();
  const serverClient = await createServerClient();

  // await しない → Streaming SSR: 結果が到着次第クライアントに反映
  prefetchSecondary(
    queryClient,
    withServerFetcher(
      $api.queryOptions('get', '/api/v1/repositories', {
        params: { query: { limit: 50 } },
      }),
      () =>
        serverClient.GET('/api/v1/repositories', {
          params: { query: { limit: 50 } },
        }),
    ),
  );

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <Suspense fallback={<RepositoriesTableSkeleton />}>
        <RepositoriesList />
      </Suspense>
    </HydrationBoundary>
  );
}
