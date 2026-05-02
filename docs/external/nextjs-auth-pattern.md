# Next.js App Router 認証パターン (GitHub OAuth + Backend API 連携)

## 要約

BoardFlow のフロントエンドは、認証処理自体を Next.js 側で行わず、Backend (Rust/Axum) の `/api/v1/auth/*` エンドポイントに委譲する。Next.js 側の責務は、Backend が設定した `boardflow_session` cookie の有無に基づく認証ガードと、未認証時のリダイレクトに限定される。

## 確認した情報

### BoardFlow の認証フロー (docs/backend/api.md Section 3)

1. `GET /api/v1/auth/login` → GitHub OAuth 画面へリダイレクト (Backend が state cookie 設定)
2. `GET /api/v1/auth/callback?code=...&state=...` → Backend が token 交換、session 作成、`boardflow_session` cookie 設定
3. `GET /api/v1/auth/me` → 現在のセッションのユーザー情報取得
4. `POST /api/v1/auth/logout` → session 削除、cookie クリア

### Next.js 側で必要な実装

#### 1. Middleware (proxy.ts / middleware.ts) での認証ガード

```typescript
import { NextRequest, NextResponse } from 'next/server'

const protectedRoutes = ['/repositories']
const publicRoutes = ['/login']

export default async function middleware(req: NextRequest) {
  const path = req.nextUrl.pathname
  const isProtectedRoute = protectedRoutes.some(route => path.startsWith(route))
  const isPublicRoute = publicRoutes.includes(path)

  // Backend が設定した session cookie の存在チェック (optimistic)
  const session = req.cookies.get('boardflow_session')?.value

  if (isProtectedRoute && !session) {
    return NextResponse.redirect(new URL('/login', req.nextUrl))
  }

  if (isPublicRoute && session) {
    return NextResponse.redirect(new URL('/repositories', req.nextUrl))
  }

  return NextResponse.next()
}

export const config = {
  matcher: ['/((?!api|_next/static|_next/image|.*\\.png$).*)'],
}
```

#### 2. ログインページ

```tsx
// app/login/page.tsx
export default function LoginPage() {
  return (
    <a href="/api/v1/auth/login">Login with GitHub</a>
  )
}
```

ログインは Backend API へのリダイレクトリンクで済む。Next.js 側に OAuth ロジックは不要。

#### 3. ユーザー情報取得 (Server Component)

```typescript
// lib/auth.ts
import { cookies } from 'next/headers'

export async function getCurrentUser() {
  const cookieStore = await cookies()
  const session = cookieStore.get('boardflow_session')

  if (!session) return null

  const res = await fetch(`${process.env.API_BASE_URL}/api/v1/auth/me`, {
    headers: {
      Cookie: `boardflow_session=${session.value}`,
    },
  })

  if (!res.ok) return null
  return res.json()
}
```

#### 4. API Proxy パターン

Next.js から Backend API を呼ぶ際、Server Components では cookie を手動で転送する必要がある。

```typescript
// lib/api/server.ts
import { cookies } from 'next/headers'

export async function serverFetch(path: string, init?: RequestInit) {
  const cookieStore = await cookies()
  const session = cookieStore.get('boardflow_session')

  return fetch(`${process.env.API_BASE_URL}${path}`, {
    ...init,
    headers: {
      ...init?.headers,
      Cookie: session ? `boardflow_session=${session.value}` : '',
    },
  })
}
```

### Next.js 16 の認証関連変更点

- Next.js 16 では `middleware.ts` の代わりに `proxy.ts` が導入された (Node.js runtime で動作)
- `cookies()` は async になった (Next.js 15+)
- Proxy は全ルートで実行され、cookie ベースの optimistic チェックに適している

## BoardFlow への示唆

- 認証ライブラリ (NextAuth.js, Better Auth 等) は **不要**。Backend が session 管理を完全に担当するため。
- Next.js 側は cookie の存在チェックと API proxy のみ担当
- middleware/proxy で cookie の存在を optimistic にチェックし、未認証ユーザーを `/login` にリダイレクト
- 実際の認可判定は Backend API が `boardflow_session` cookie を検証して行う
- Server Components でのデータフェッチ時は `cookies()` から session cookie を取得して Backend API に転送

## 採用判断

**採用**: Backend API 連携パターンで実装する。Auth ライブラリは使用しない。

## 制約と pitfall

1. **Cookie の転送**: Server Components から Backend API を呼ぶ際、cookie は自動転送されない。`cookies()` API で手動取得して `Cookie` header に設定する必要がある。
2. **Middleware vs Proxy**: Next.js 16 では `proxy.ts` が推奨。Edge Runtime 制約なしで Node.js runtime で動作する。ただし Next.js 15 を使う場合は `middleware.ts` を使用。
3. **CORS**: Next.js と Backend API が異なるオリジンの場合、CORS 設定が必要。同一オリジンにリバースプロキシするか、Next.js の `rewrites` で `/api/v1/*` を Backend にプロキシする方法が簡単。
4. **Layout の認証チェック**: Layout は Partial Rendering により navigation 時に再レンダリングされない。認証チェックは page component またはデータフェッチ層で行うべき。

## 未解決の疑問

- Next.js と Backend API のデプロイ構成（同一ドメインかどうか）によって cookie とプロキシ戦略が変わる
- Next.js 15 と 16 のどちらを使うか（16 は 2025年5月時点で Latest だが安定性の確認が必要）

## 参照URL

- https://nextjs.org/docs/app/guides/authentication
- https://nextjs.org/docs/app/api-reference/functions/cookies
- https://nextjs.org/docs/app/api-reference/file-conventions/proxy
