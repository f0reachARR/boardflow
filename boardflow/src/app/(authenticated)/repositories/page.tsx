import { dehydrate, HydrationBoundary } from "@tanstack/react-query"
import { getQueryClient } from "@/lib/query-client"
import { createServerClient } from "@/lib/api/server"
import { $api } from "@/lib/api/react-query"
import { RepositoriesList } from "@/components/repositories/repositories-list"

export default async function RepositoriesPage() {
  const queryClient = getQueryClient()
  const serverClient = await createServerClient()

  const options = $api.queryOptions("get", "/api/v1/repositories", {
    params: { query: { limit: 50 } },
  })

  await queryClient.prefetchQuery({
    ...options,
    queryFn: async () => {
      const { data, error } = await serverClient.GET("/api/v1/repositories", {
        params: { query: { limit: 50 } },
      })
      if (error) throw new Error("Failed to fetch repositories")
      return data
    },
  })

  return (
    <HydrationBoundary state={dehydrate(queryClient)}>
      <RepositoriesList />
    </HydrationBoundary>
  )
}
