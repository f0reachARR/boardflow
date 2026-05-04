# Next.js Middleware: Cookie削除・リダイレクトループ防止・ログイン後リダイレクト

## 要約

Next.js middleware で無効セッションCookie起因のリダイレクトループを解消するための手法と、ログイン後に元ページへリダイレクトするパターンを調査した。middleware 内では `NextResponse.redirect()` で生成したレスポンスに対して `.cookies.delete()` を呼ぶことで Cookie 削除とリダイレクトを同時に実行できる。middleware 内での API 呼び出しはパフォーマンス・Edge Runtime 制約の観点から推奨されず、Cookie の「存在チェック + layout での有効性検証 + 無効時の Cookie クリア」が現実的なアプローチ。

## 確認した情報

### 1. middleware 内での Cookie 削除 + リダイレクトの同時実行

Next.js `NextResponse` API では、`redirect()` で生成した Response オブジェクトに対して `.cookies.delete(name)` を呼べる。これにより `Set-Cookie` ヘッダー付きのリダイレクトレスポンスが生成される。

```typescript
// Cookie削除 + リダイレクトの同時実行パターン
const response = NextResponse.redirect(new URL('/login', request.url));
response.cookies.delete('boardflow_session');
return response;
```

**参照**: https://nextjs.org/docs/app/api-reference/functions/next-response

**注意点**:
- `response.cookies.delete()` は内部的に `Set-Cookie: boardflow_session=; Path=/; Max-Age=0` を設定する
- Domain 属性が設定されている場合、削除時にも同じ Domain を指定する必要がある（BoardFlow はバックエンドが同一ドメインで `Path=/` のみ設定しているため該当しない）
- middleware で削除した Cookie は、同一リクエスト内の Server Component からは **次のリクエストから** 反映される（vercel/next.js#49442）

### 2. middleware 内での API 呼び出しによるセッション検証

**推奨されない理由**:
- middleware は Vercel Edge Runtime（またはデフォルト Edge 互換モード）で実行されるため、外部 API 呼び出しはレイテンシとコストが発生する
- すべてのリクエストで API コールが発生するため、バックエンドへの負荷が大きい
- 一般的なベストプラクティス: 「middleware では Cookie の存在チェック（optimistic）のみ。実際のセッション有効性検証は Server Component/layout で行う」
- Better Auth 公式ドキュメントも「In Next.js middleware, it's recommended to only check for the existence of a session cookie to handle redirection」と明言

**推奨パターン**:
- middleware: Cookie 存在チェック（軽量・高速）
- layout/Server Component: API 呼び出しでセッション有効性を検証
- **無効セッション検出時**: Server Component 側で Cookie をクリアしてリダイレクト

### 3. リダイレクトループの根本原因と対策

BoardFlow のリダイレクトループのメカニズム:
1. ユーザーが無効な `boardflow_session` Cookie を持っている
2. middleware: Cookie 存在 → 通過（`/login` アクセス時は `/repositories` にリダイレクト）
3. layout: `getCurrentUser()` → API 401 → `null` → `redirect('/login')`
4. middleware: Cookie 存在 + `/login` → `/repositories` にリダイレクト
5. 2-4 のループ

**対策案（推奨）**: middleware の `/login` ガードで Cookie の「存在」だけでなく、`getCurrentUser()` が失敗した場合に Cookie を削除する仕組みを導入。具体的には:

**案A: layout で Cookie 削除してからリダイレクト（推奨）**
```typescript
// (authenticated)/layout.tsx
import { cookies } from 'next/headers';
import { redirect } from 'next/navigation';

export default async function AuthenticatedLayout({ children }) {
  const user = await getCurrentUser();
  if (!user) {
    // 無効Cookieを削除してからリダイレクト
    const cookieStore = await cookies();
    cookieStore.delete('boardflow_session');
    redirect('/login');
  }
  return <AppShell user={user}>{children}</AppShell>;
}
```

→ Cookie 削除後のリダイレクトで middleware が Cookie なしの状態で `/login` を処理するのでループしない。

**注意**: `cookies().delete()` は Server Action か Route Handler でのみ使用可能。Server Component（layout.tsx）では `cookies()` は読み取り専用。

**案B: middleware で `/login` の保護条件を変更（よりシンプル・推奨）**
```typescript
// middleware.ts
// /login へのアクセスは常に通す（Cookie有無問わず）
// Cookie有 + /login の場合にリダイレクトしない
if (PUBLIC_PATHS.some((p) => pathname.startsWith(p))) {
  return NextResponse.next();
}
```

→ `/login` ページ自体が認証済みユーザーの検知とリダイレクトを担当。middleware は `/login` を常にパブリックとして扱う。layout でセッション無効 → `/login` リダイレクト → middleware 通過 → ログインページ表示。

**案C: middleware でレスポンス Cookie を削除してリダイレクト（ハイブリッド）**

layout.tsx の Server Component では `cookies().delete()` が使えないため、以下の流れが現実的:

1. layout.tsx: `getCurrentUser()` が null → クエリパラメータ付きでリダイレクト `redirect('/login?session_expired=1')`
2. middleware: `/login?session_expired=1` を検出 → Cookie 削除 + `/login` リダイレクト
3. 以降のリクエストは Cookie なし → ループしない

ただしこれは複雑すぎるため、**案B が最もシンプルで推奨**。

### 4. ログイン後の元ページリダイレクト（redirect_to パターン）

**フロントエンド側（middleware + login ページ）**:

```typescript
// middleware.ts - 未認証時に元URLをクエリパラメータに付与
if (!session) {
  const loginUrl = new URL('/login', request.url);
  loginUrl.searchParams.set('redirect_to', pathname + request.nextUrl.search);
  return NextResponse.redirect(loginUrl);
}
```

```tsx
// login/page.tsx - redirect_toをログインリンクに伝播
export default function LoginPage({ searchParams }) {
  const redirectTo = searchParams.redirect_to || '/repositories';
  const loginHref = `/api/v1/auth/login?redirect_to=${encodeURIComponent(redirectTo)}`;
  return <Link href={loginHref}>Sign in with GitHub</Link>;
}
```

**バックエンド側（auth.rs）**:

```rust
// login handler: redirect_to を OAuth state cookie に埋め込む
// （または別の Cookie に保存）
// callback handler: state cookie から redirect_to を取り出してリダイレクト先に使用
```

**伝搬フロー**:
1. middleware → `/login?redirect_to=/repositories/123`
2. login ページ → `/api/v1/auth/login?redirect_to=/repositories/123`
3. backend login handler → `boardflow_redirect_to` Cookie に保存（HttpOnly, SameSite=Lax, Max-Age=300）
4. GitHub OAuth → callback
5. backend callback handler → Cookie から `redirect_to` を読み取り → バリデーション後にリダイレクト

### 5. Open Redirect 防止（redirect_to バリデーション）

**OWASP 推奨**: Open Redirect は OWASP Top 10 2025 の A01:2025 (Broken Access Control) に分類。

**バリデーション手法**:

```typescript
// フロントエンド側（TypeScript）
function isValidRedirectPath(path: string): boolean {
  // 相対パスのみ許可、プロトコル・ドメインを含む URL を拒否
  if (!path.startsWith('/')) return false;
  if (path.startsWith('//')) return false;  // protocol-relative URL
  if (path.includes('://')) return false;   // absolute URL
  if (path.includes('\\')) return false;    // backslash bypass
  // 不正な文字を含まないことを確認
  try {
    const url = new URL(path, 'http://dummy');
    return url.pathname === path.split('?')[0]; // パスが変更されていないことを確認
  } catch {
    return false;
  }
  return true;
}
```

```rust
// バックエンド側（Rust）
fn validate_redirect_to(path: &str) -> Option<&str> {
    // 相対パスのみ許可
    if !path.starts_with('/') { return None; }
    if path.starts_with("//") { return None; }
    if path.contains("://") { return None; }
    if path.contains('\\') { return None; }
    // URL デコード後の再チェック
    let decoded = urlencoding::decode(path).ok()?;
    if decoded.starts_with("//") || decoded.contains("://") { return None; }
    Some(path)
}
```

**重要なバイパス手法（防御すべきもの）**:
- `//evil.com` — protocol-relative URL
- `/\evil.com` — backslash bypass
- `/%2Fevil.com` — URL エンコードによるバイパス
- `https://evil.com` — 絶対 URL
- `javascript:alert(1)` — JavaScript scheme
- `/login@evil.com` — userinfo 部分を使ったバイパス

### 6. middleware リダイレクトキャッシュ問題

`x-middleware-cache: no-cache` ヘッダーを設定してブラウザキャッシュによるリダイレクトループを防ぐパターンが報告されている。Next.js 15 ではデフォルトでキャッシュされないが、念のため設定を推奨。

```typescript
const response = NextResponse.redirect(new URL('/login', request.url));
response.headers.set('x-middleware-cache', 'no-cache');
return response;
```

## BoardFlow への示唆

### ループ修正（3箇所の修正）

1. **middleware.ts**: `/login` アクセスを常にパブリックとして扱う（Cookie 有無に関わらず通過）。または: Cookie 有 + `/login` 時のリダイレクトを削除し、login ページ側で認証済みユーザーをリダイレクト
2. **(authenticated)/layout.tsx**: `getCurrentUser()` が null の場合、Server Action 経由で Cookie を削除するか、クエリパラメータ付きでリダイレクト
3. **login/page.tsx**: 認証済みユーザー（Cookieの有効性確認後）のリダイレクト処理を追加（オプション）

### ログイン後リダイレクト

1. **middleware.ts**: 未認証リダイレクト時に `redirect_to` クエリパラメータを付与
2. **login/page.tsx**: `redirect_to` を `/api/v1/auth/login` に伝播
3. **auth.rs login handler**: `redirect_to` を Cookie（`boardflow_redirect_to`）に保存
4. **auth.rs callback handler**: Cookie から `redirect_to` を読み取り、バリデーション後にリダイレクト先として使用

## 採用/不採用判断

- **案B（middleware で /login を常にパブリック化）**: **採用** — 最もシンプルでリグレッションリスクが低い
- **redirect_to パターン**: **採用** — middleware → login → backend login → backend callback の 4 段階で伝播
- **redirect_to の Open Redirect 防止**: **必須** — バックエンド callback で相対パスバリデーション実装
- **middleware 内 API 呼び出し**: **不採用** — パフォーマンス問題、Edge Runtime 制約

## 制約と pitfall

1. Server Component（layout.tsx）からは `cookies().delete()` が直接使えない（読み取り専用）。Server Action を経由するか、middleware で対処する必要がある
2. `boardflow_redirect_to` Cookie は短い TTL（300 秒）を設定し、使用後に即削除すること
3. middleware で削除した Cookie は同一リクエスト内の Server Component には即座に反映されない（次リクエストから）
4. Next.js の 307 リダイレクトはブラウザにキャッシュされる場合がある — `x-middleware-cache: no-cache` を設定推奨
5. OAuth state と redirect_to を分離すること（state はCSRF防御用、redirect_to はUX用）

## 未解決の疑問

1. Next.js 15 以降で `cookies().delete()` が Server Component から使えるようになるか（現時点では Server Action/Route Handler のみ）
2. `boardflow_redirect_to` Cookie をバックエンドで管理するか、フロントエンドで sessionStorage 等で管理するかの選択（Cookie 方式が OAuth フローとの相性で推奨）

## 参照URL

- https://nextjs.org/docs/app/api-reference/functions/next-response
- https://nextjs.org/docs/app/api-reference/functions/cookies
- https://cheatsheetseries.owasp.org/cheatsheets/Unvalidated_Redirects_and_Forwards_Cheat_Sheet.html
- https://github.com/vercel/next.js/issues/49442
- https://itnext.io/fixing-next-js-authentication-redirects-preserving-deep-links-after-login-1d3118765e31
- https://richardkovacs.dev/blog/deleting-cookies-in-nextjs-middleware
