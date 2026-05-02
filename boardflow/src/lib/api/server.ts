import createClient from "openapi-fetch"
import { cookies } from "next/headers"
import type { paths } from "./schema"

export async function createServerClient() {
  const cookieStore = await cookies()
  const session = cookieStore.get("boardflow_session")

  return createClient<paths>({
    baseUrl: "",
    headers: {
      ...(session ? { Cookie: `boardflow_session=${session.value}` } : {}),
    },
  })
}
