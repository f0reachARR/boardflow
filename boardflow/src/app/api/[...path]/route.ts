import { type NextRequest, NextResponse } from 'next/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const HOP_BY_HOP_REQUEST_HEADERS = new Set([
  'host',
  'connection',
  'content-length',
  'transfer-encoding',
  'keep-alive',
  'proxy-authenticate',
  'proxy-authorization',
  'te',
  'trailer',
  'upgrade',
]);

const HOP_BY_HOP_RESPONSE_HEADERS = new Set([
  'connection',
  'content-encoding',
  'content-length',
  'transfer-encoding',
  'keep-alive',
  'proxy-authenticate',
  'proxy-authorization',
  'te',
  'trailer',
  'upgrade',
]);

const METHODS_WITHOUT_BODY = new Set(['GET', 'HEAD']);

async function proxy(request: NextRequest): Promise<NextResponse> {
  const backend = process.env.API_BASE_URL ?? 'http://localhost:3000';
  const targetUrl = `${backend}${request.nextUrl.pathname}${request.nextUrl.search}`;

  const requestHeaders = new Headers();
  request.headers.forEach((value, key) => {
    if (!HOP_BY_HOP_REQUEST_HEADERS.has(key.toLowerCase())) {
      requestHeaders.set(key, value);
    }
  });

  const body = METHODS_WITHOUT_BODY.has(request.method) ? undefined : await request.arrayBuffer();

  const upstream = await fetch(targetUrl, {
    method: request.method,
    headers: requestHeaders,
    body,
    redirect: 'manual',
    cache: 'no-store',
  });

  const responseHeaders = new Headers();
  upstream.headers.forEach((value, key) => {
    const lower = key.toLowerCase();
    if (HOP_BY_HOP_RESPONSE_HEADERS.has(lower) || lower === 'set-cookie') {
      return;
    }
    responseHeaders.set(key, value);
  });

  const setCookies = upstream.headers.getSetCookie?.() ?? [];
  for (const cookie of setCookies) {
    responseHeaders.append('set-cookie', cookie);
  }

  return new NextResponse(upstream.body, {
    status: upstream.status,
    statusText: upstream.statusText,
    headers: responseHeaders,
  });
}

export const GET = proxy;
export const POST = proxy;
export const PUT = proxy;
export const PATCH = proxy;
export const DELETE = proxy;
export const OPTIONS = proxy;
export const HEAD = proxy;
