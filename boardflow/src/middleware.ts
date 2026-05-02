import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"

const PUBLIC_PATHS = ["/login"]

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl
  const session = request.cookies.get("boardflow_session")

  // Root redirect
  if (pathname === "/") {
    if (session) {
      return NextResponse.redirect(new URL("/repositories", request.url))
    }
    return NextResponse.redirect(new URL("/login", request.url))
  }

  // Authenticated user accessing login → redirect to repositories
  if (pathname === "/login" && session) {
    return NextResponse.redirect(new URL("/repositories", request.url))
  }

  // Public paths don't require auth
  if (PUBLIC_PATHS.some((p) => pathname.startsWith(p))) {
    return NextResponse.next()
  }

  // Unauthenticated → redirect to login
  if (!session) {
    return NextResponse.redirect(new URL("/login", request.url))
  }

  return NextResponse.next()
}

export const config = {
  matcher: [
    "/((?!_next/static|_next/image|favicon.ico|api/).*)",
  ],
}
