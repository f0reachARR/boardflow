import type { NextRequest } from 'next/server';
import { NextResponse } from 'next/server';

const PUBLIC_PATHS = ['/login'];

export function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;
  const session = request.cookies.get('boardflow_session');

  // Root redirect
  if (pathname === '/') {
    if (session) {
      return NextResponse.redirect(new URL('/repositories', request.url));
    }
    return NextResponse.redirect(new URL('/login', request.url));
  }

  // Public paths always accessible (no session check for /login to prevent redirect loop)
  if (PUBLIC_PATHS.some((p) => pathname === p || pathname.startsWith(`${p}/`))) {
    return NextResponse.next();
  }

  // Unauthenticated → redirect to login with redirect_to
  if (!session) {
    const loginUrl = new URL('/login', request.url);
    const redirectTo = pathname + request.nextUrl.search;
    if (redirectTo !== '/') {
      loginUrl.searchParams.set('redirect_to', redirectTo);
    }
    const response = NextResponse.redirect(loginUrl);
    response.headers.set('x-middleware-cache', 'no-cache');
    return response;
  }

  return NextResponse.next();
}

export const config = {
  matcher: ['/((?!_next/static|_next/image|favicon.ico|api/).*)'],
};
