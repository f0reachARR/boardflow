import type { QueryClient } from '@tanstack/react-query';
import { notFound } from 'next/navigation';

/**
 * Primary resource: await して fetchQuery し、取得失敗なら notFound()。
 */
export async function fetchPrimary<T>(
  queryClient: QueryClient,
  options: { queryKey: readonly unknown[]; queryFn: () => Promise<T> },
): Promise<T> {
  const result = await queryClient.fetchQuery(options).catch(() => null);
  if (!result) {
    notFound();
  }
  return result;
}

/**
 * Secondary resource: prefetchQuery (await しない → Streaming SSR)。
 */
export function prefetchSecondary(
  queryClient: QueryClient,
  options: { queryKey: readonly unknown[]; queryFn: () => Promise<unknown> },
): void {
  queryClient.prefetchQuery(options);
}

/**
 * $api.queryOptions() の結果に、serverClient を使った queryFn を上書きする。
 */
export function withServerFetcher<T>(
  clientOptions: { queryKey: readonly unknown[] },
  serverFetcher: () => Promise<{ data?: T; error?: unknown }>,
  errorMessage?: string,
): { queryKey: readonly unknown[]; queryFn: () => Promise<T> } {
  return {
    queryKey: clientOptions.queryKey,
    queryFn: async () => {
      const { data, error } = await serverFetcher();
      if (error) throw errorMessage ? new Error(errorMessage) : error;
      return data as T;
    },
  };
}
