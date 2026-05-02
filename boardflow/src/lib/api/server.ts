import createClient from "openapi-fetch"
import { cookies } from "next/headers"
import type { paths } from "./schema"

const API_BASE_URL = process.env.API_BASE_URL ?? "http://localhost:3001"

export async function createServerClient() {
  const cookieStore = await cookies()
  const session = cookieStore.get("boardflow_session")

  return createClient<paths>({
    baseUrl: API_BASE_URL,
    headers: {
      ...(session ? { Cookie: `boardflow_session=${session.value}` } : {}),
    },
  })
}
