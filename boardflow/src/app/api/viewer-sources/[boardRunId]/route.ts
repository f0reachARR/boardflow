import { NextRequest, NextResponse } from "next/server"

const API_BASE_URL = process.env.API_BASE_URL ?? "http://localhost:3001"

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ boardRunId: string }> }
) {
  const { boardRunId } = await params
  const session = request.cookies.get("boardflow_session")

  if (!session) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 })
  }

  const res = await fetch(
    `${API_BASE_URL}/api/v1/board-runs/${encodeURIComponent(boardRunId)}/viewer-sources`,
    {
      headers: {
        Cookie: `boardflow_session=${session.value}`,
      },
    }
  )

  if (!res.ok) {
    return NextResponse.json(
      { error: "Failed to fetch viewer sources" },
      { status: res.status }
    )
  }

  const data = await res.json()
  return NextResponse.json(data)
}
