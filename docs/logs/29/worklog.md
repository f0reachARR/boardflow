# Issue #29: Next.js + Chakra UI v3 フロントエンドセットアップ 作業ログ

## Issue までの経緯

- BoardFlow の Backend (Rust/Axum) は概ね実装済み
- `boardflow/` ディレクトリにフロントエンドプロジェクトをセットアップする Issue
- `docs/frontend/summary.md` にフロントエンド仕様が定義済み
- `docs/technology.md` で Next.js App Router + Chakra UI + lucide-react が技術選定済み

## ユーザー要望

- docs 以下の仕様に基づいてフロントエンドアプリケーションを一通り実装する
- GitHub OAuth 認証フロー、基本的なレイアウト、API クライアントの基盤を含む

## 調査結果 (2026-05-02)

### 1. Chakra UI v3 + Next.js App Router セットアップ

- **現在の最新バージョン**: Chakra UI v3.35.0
- **必要パッケージ**: `@chakra-ui/react`, `@emotion/react`
- **v2 から不要になったもの**: `framer-motion`, `@chakra-ui/next-js`, `@emotion/styled`
- **Snippets システム**: `npx @chakra-ui/cli snippet add` で UI コンポーネントが `components/ui/` に生成される
- **Provider**: snippets の `components/ui/provider` を `app/layout.tsx` に配置
- **重要な pitfall**: Turbopack で hydration エラー → `--webpack` フラグ必須
- 詳細: `docs/external/chakra-ui-v3-nextjs-setup.md`

### 2. Next.js App Router の認証パターン

- **方針**: Auth ライブラリは不要。Backend が session を完全管理
- **Frontend の責務**: `boardflow_session` cookie の存在チェックのみ
- **proxy.ts** (Next.js 16): cookie 有無で `/login` へリダイレクト (optimistic check)
- **Server Component**: `cookies()` から session を取得し Backend API に転送
- **ログインフロー**: `/api/v1/auth/login` への単純リダイレクト
- 詳細: `docs/external/nextjs-auth-pattern.md`

### 3. openapi-typescript + openapi-fetch

- **型生成**: `npx openapi-typescript <spec-url> -o ./lib/api/schema.d.ts`
- **API クライアント**: `openapi-fetch` (6 kB) で型安全な fetch
- **使用パターン**: `client.GET("/path", { params: {...} })` で型推論が効く
- **Server Component 対応**: cookie 手動転送が必要
- 詳細: `docs/external/openapi-typescript-fetch.md`

### 4. lucide-react

- **インストール**: `npm install lucide-react`
- **使い方**: `import { Icon } from 'lucide-react'` で tree-shake される
- **BoardFlow 向けアイコン**: ステータス表示、artifact 種別、Git 関連等
- 詳細: `docs/external/lucide-react-setup.md`

### 5. Next.js 16 proxy.ts (追加調査 2026-05-02)

- **Next.js 16.0.0** で `middleware.ts` → `proxy.ts` にリネーム (非推奨化)
- API は同一: `NextRequest`, `NextResponse`, `cookies` アクセス
- **Node.js runtime** がデフォルト (Edge Runtime ではない)
- 認証の最終判定は proxy ではなく Data Access Layer / Server Functions で行うべき
- Optimistic な cookie 存在チェック + リダイレクトは proxy で問題なし
- `matcher` config もそのまま使える
- `middleware.ts` も後方互換で動作するが、新規プロジェクトでは `proxy.ts` を使用

---

## 計画 (2026-05-02)

### 判定: `implementation_required`

### 目的

- `boardflow/` に Next.js 16 + TypeScript + Chakra UI v3 のプロジェクトをセットアップする
- GitHub OAuth 認証フロー (Backend API 連携) を実装する
- 基本的なアプリケーションレイアウト (Header, Sidebar等) を構築する
- openapi-typescript + openapi-fetch による型安全 API クライアント基盤を構築する
- Next.js rewrites で Backend API をプロキシする設定を行う

### 非目的

- 画面の実装 (Repository一覧/詳細、BoardProject詳細、Run一覧/詳細 等) — 別 Issue
- E2E テスト設定 (Playwright) — 別 Issue
- KiCanvas 統合 — 別 Issue
- Token 管理画面 — 別 Issue
- iBOM/PDF/SVG artifact 表示 — 別 Issue

### 受け入れ条件

1. `boardflow/` で `npm run dev` → Next.js dev server が起動する
2. Chakra UI の Provider が設定され、コンポーネントが表示される
3. `proxy.ts` が未認証ユーザーを `/login` にリダイレクトする
4. ログインページから `/api/v1/auth/login` (Backend) へリダイレクトできる
5. ログイン後、`GET /api/v1/auth/me` でユーザー情報を取得・表示できる
6. ログアウトが動作する
7. Backend API の OpenAPI spec から TypeScript 型が生成される
8. Server Component 用 / Client Component 用の API クライアントが使える
9. `next.config.ts` の rewrites で `/api/v1/*` が Backend にプロキシされる
10. CI で lint / type-check / build が通る

### 詳細要件

#### Phase 1: プロジェクト初期化

| ファイル | 内容 |
|---|---|
| `boardflow/package.json` | Next.js 16, TypeScript, 依存関係定義 |
| `boardflow/tsconfig.json` | strict mode, path alias (`@/*` → `./src/*`) |
| `boardflow/next.config.ts` | rewrites, optimizePackageImports, webpack フラグ |
| `boardflow/.env.local.example` | `API_BASE_URL=http://localhost:3001` |
| `boardflow/.gitignore` | Next.js 標準 |
| `boardflow/src/app/layout.tsx` | RootLayout + Chakra Provider |
| `boardflow/src/components/ui/provider.tsx` | Chakra UI Provider (snippets) |

**依存関係:**
```json
{
  "dependencies": {
    "next": "^16",
    "react": "^19",
    "react-dom": "^19",
    "@chakra-ui/react": "^3.35",
    "@emotion/react": "^11",
    "openapi-fetch": "^0.13",
    "lucide-react": "^0.500"
  },
  "devDependencies": {
    "typescript": "^5",
    "@types/react": "^19",
    "@types/react-dom": "^19",
    "openapi-typescript": "^7",
    "@chakra-ui/cli": "^3",
    "eslint": "^9",
    "eslint-config-next": "^16"
  }
}
```

**npm scripts:**
```json
{
  "dev": "next dev --webpack",
  "build": "next build --webpack",
  "start": "next start",
  "lint": "next lint",
  "typecheck": "tsc --noEmit",
  "generate:api": "openapi-typescript http://localhost:3001/api/v1/openapi.json -o ./src/lib/api/schema.d.ts"
}
```

#### Phase 2: 認証基盤

| ファイル | 内容 |
|---|---|
| `boardflow/src/proxy.ts` | 認証ガード (cookie 有無チェック) |
| `boardflow/src/lib/auth.ts` | `getCurrentUser()` ヘルパー |
| `boardflow/src/app/login/page.tsx` | ログインページ (GitHub OAuth リダイレクト) |

**proxy.ts の実装方針:**
```typescript
import { NextResponse } from 'next/server'
import type { NextRequest } from 'next/server'

const protectedPaths = ['/repositories', '/']
const publicPaths = ['/login']

export function proxy(request: NextRequest) {
  const { pathname } = request.nextUrl
  const session = request.cookies.get('boardflow_session')?.value

  // ルート → /repositories リダイレクト
  if (pathname === '/') {
    if (!session) return NextResponse.redirect(new URL('/login', request.url))
    return NextResponse.redirect(new URL('/repositories', request.url))
  }

  // 保護対象ルートで未認証 → /login
  if (!session && !publicPaths.some(p => pathname.startsWith(p))) {
    return NextResponse.redirect(new URL('/login', request.url))
  }

  // 認証済みでログインページ → /repositories
  if (session && pathname === '/login') {
    return NextResponse.redirect(new URL('/repositories', request.url))
  }

  return NextResponse.next()
}

export const config = {
  matcher: ['/((?!_next/static|_next/image|favicon.ico|.*\\.png$).*)'],
}
```

**next.config.ts の rewrites:**
```typescript
const nextConfig = {
  experimental: {
    optimizePackageImports: ['@chakra-ui/react'],
  },
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: `${process.env.API_BASE_URL ?? 'http://localhost:3001'}/api/:path*`,
      },
    ]
  },
}
```

#### Phase 3: API クライアント基盤

| ファイル | 内容 |
|---|---|
| `boardflow/src/lib/api/schema.d.ts` | OpenAPI 生成型 (gitignore 対象) |
| `boardflow/src/lib/api/server.ts` | Server Component 用クライアント (cookie転送) |
| `boardflow/src/lib/api/client.ts` | Client Component 用クライアント (ブラウザcookie自動送信) |

**Server Component 用 (lib/api/server.ts):**
```typescript
import createClient from 'openapi-fetch'
import type { paths } from './schema'
import { cookies } from 'next/headers'

export async function createServerClient() {
  const cookieStore = await cookies()
  const session = cookieStore.get('boardflow_session')

  return createClient<paths>({
    baseUrl: '',  // rewrites で同一オリジン
    headers: {
      Cookie: session ? `boardflow_session=${session.value}` : '',
    },
  })
}
```

**Client Component 用 (lib/api/client.ts):**
```typescript
import createClient from 'openapi-fetch'
import type { paths } from './schema'

// ブラウザの cookie は自動送信 (same-origin)
export const apiClient = createClient<paths>({
  baseUrl: '',
  credentials: 'same-origin',
})
```

#### Phase 4: 基本レイアウト

| ファイル | 内容 |
|---|---|
| `boardflow/src/components/layout/header.tsx` | ヘッダー (ロゴ, ユーザーメニュー, ログアウト) |
| `boardflow/src/components/layout/sidebar.tsx` | サイドバー (ナビゲーション) |
| `boardflow/src/components/layout/app-shell.tsx` | AppShell (Header + Sidebar + Content) |
| `boardflow/src/app/(authenticated)/layout.tsx` | 認証済みルート用レイアウト |
| `boardflow/src/app/(authenticated)/repositories/page.tsx` | プレースホルダー (一覧画面の枠) |

**ディレクトリ構造 (最終形):**
```
boardflow/
  src/
    app/
      layout.tsx                    # RootLayout (Chakra Provider)
      login/
        page.tsx                    # ログインページ
      (authenticated)/
        layout.tsx                  # AppShell (Header + Sidebar)
        repositories/
          page.tsx                  # プレースホルダー
          [repositoryId]/
            page.tsx                # (別 Issue)
            boards/
              [boardProjectId]/
                page.tsx            # (別 Issue)
                runs/
                  page.tsx          # (別 Issue)
                  [boardRunId]/
                    page.tsx        # (別 Issue)
    components/
      layout/
        header.tsx
        sidebar.tsx
        app-shell.tsx
      ui/
        provider.tsx                # Chakra snippets
        (他の snippets)
    lib/
      api/
        schema.d.ts                 # 生成ファイル (gitignore)
        server.ts                   # Server Component 用
        client.ts                   # Client Component 用
      auth.ts                       # getCurrentUser()
    proxy.ts                        # 認証ガード
  next.config.ts
  tsconfig.json
  package.json
  .env.local.example
  .gitignore
```

### 影響範囲

- **新規追加**: `boardflow/` ディレクトリ全体 (新規プロジェクト)
- **CI変更**: `.github/workflows/ci.yml` に frontend job を追加
- **既存コード変更**: なし (Backend は変更不要)

### 設計方針

1. **Server Components 優先**: データフェッチは Server Component、UI表示で Chakra が必要な部分のみ Client Component
2. **認証は optimistic check**: proxy.ts での cookie 存在チェックは optimistic。実際の認可は Backend API が担当
3. **型安全**: openapi-typescript で Backend API と型を同期
4. **rewrites パターン**: `/api/v1/*` を Backend にプロキシし、CORS 問題を回避
5. **Turbopack 非互換対応**: dev/build で `--webpack` フラグを使用
6. **Route Groups**: `(authenticated)` でレイアウトを分離し、認証済み画面は共通レイアウト適用

### テスト観点

- [ ] `npm run build` が成功する (型エラーなし)
- [ ] `npm run lint` が通る
- [ ] `npm run typecheck` が通る
- [ ] proxy.ts: 未認証リクエスト → /login リダイレクト
- [ ] proxy.ts: 認証済み + /login アクセス → /repositories リダイレクト
- [ ] ログインボタン → Backend `/api/v1/auth/login` へ遷移
- [ ] rewrites: `/api/v1/auth/me` が Backend にプロキシされる
- [ ] (手動確認) Backend起動状態で認証フロー全体が動作する

### ドキュメント更新対象

- `docs/external/nextjs-auth-pattern.md`: proxy.ts (Next.js 16) への更新反映 (完了時)
- CI 設定変更箇所のコメント
- `boardflow/README.md`: 開発手順記載

### CI 変更計画

`.github/workflows/ci.yml` に `frontend` job を追加:

```yaml
  frontend:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: boardflow
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'
          cache-dependency-path: boardflow/package-lock.json
      - run: npm ci
      - run: npm run lint
      - run: npm run typecheck
      - run: npm run build
```

※ `generate:api` は Backend 起動が必要なため、CI では事前コミットした `schema.d.ts` を使用するか、別途生成ステップを検討

### 実装上の注意点

1. **`schema.d.ts` の管理**: OpenAPI spec から生成。初期段階では Backend を起動して生成し、コミットに含める。将来的には CI で自動生成を検討
2. **Chakra UI Snippets**: `npx @chakra-ui/cli snippet add` は対話的なので、必要な snippets のみ追加する
3. **`suppressHydrationWarning`**: `<html>` タグに必須 (next-themes 起因)
4. **Node.js 20.x**: `openapi-typescript` CLI と Next.js 16 の最低要件
5. **env 分離**: `API_BASE_URL` は server-only 環境変数。Client Component は rewrites 経由で同一オリジンアクセス
6. **proxy.ts の matcher**: `_next/static`, `_next/image`, `favicon.ico` を除外

### 未解決の疑問

- **OpenAPI spec の初期生成**: Backend が `/api/v1/openapi.json` を提供しているので、開発時に Backend を起動して生成する。CI では schema.d.ts をコミットに含めるか、Backend を CI で起動して生成するか → 初期は**コミットに含める方針**で進める
- **Chakra UI カスタムテーマ**: MVP では デフォルトテーマ使用。ブランドカラー等のカスタマイズは後回し

### 残リスク

- Next.js 16 + Chakra UI v3 の組み合わせが Turbopack 非互換であることは確認済み。`--webpack` で回避。ビルド速度への影響はMVPでは許容
- `proxy.ts` は Next.js 16 の新規命名であり、エコシステムのドキュメントが middleware.ts ベースのものが多い。動作確認は手動で行う
- openapi-fetch の型推論は OpenAPI spec の品質に依存。Backend の utoipa 設定が不完全な場合、型が `unknown` になる可能性あり

### 推奨パッケージまとめ

```json
{
  "dependencies": {
    "@chakra-ui/react": "^3.35.0",
    "@emotion/react": "^11",
    "lucide-react": "latest",
    "next": "^15",
    "next-themes": "latest",
    "openapi-fetch": "latest",
    "react": "^19",
    "react-dom": "^19"
  },
  "devDependencies": {
    "@chakra-ui/cli": "latest",
    "openapi-typescript": "latest",
    "typescript": "^5"
  }
}
```

### Next.js バージョン選定

- Next.js 16 は 2025年4月リリースだが、Chakra UI 公式が "works with Next.js 15 and 16" と明記
- Turbopack 互換問題のため `--webpack` フラグが必要な点は 15/16 共通
- 安定性を考慮し **Next.js 15** を推奨。16 は proxy.ts 等の新機能があるが、Chakra UI の sandbox テンプレートも 15 ベース

## 計画

実装エージェントが以下を順に実施:

1. `boardflow/` に Next.js 15 プロジェクトを `create-next-app` で初期化
2. Chakra UI v3 インストール + snippets 追加
3. `next.config.mjs` で `optimizePackageImports` 設定
4. `app/layout.tsx` に Provider 配置
5. middleware.ts で認証ガード実装
6. `lib/api/` に openapi-fetch ベースの API クライアント基盤構築
7. ログインページ (`/login`)
8. Repository 一覧・詳細画面
9. BoardProject / BoardRun 関連画面
10. Artifact 表示関連

## 結論ステータス

**`implementation_required`**: 調査は完了。全トピックについて十分な情報が揃っており、実装に進める。

## 残リスク

- Turbopack hydration エラーの `--webpack` workaround が将来的にどう解消されるか不明
- Next.js 15 vs 16 の選択は実装時に再確認が必要
- Backend API の OpenAPI spec エンドポイントの確認が必要（型生成のため）
- Next.js と Backend API のデプロイ構成（同一ドメイン vs 別ドメイン）が Cookie 戦略に影響

## 更新したファイル

- `docs/external/chakra-ui-v3-nextjs-setup.md` (新規)
- `docs/external/nextjs-auth-pattern.md` (新規)
- `docs/external/openapi-typescript-fetch.md` (新規)
- `docs/external/lucide-react-setup.md` (新規)
- `docs/logs/29/worklog.md` (本ファイル、新規)

---

## 実装結果 (2026-05-02)

### 実装完了

計画を基に `boardflow/` ディレクトリにフロントエンドプロジェクトを構築完了。

#### 計画からの変更点

1. **Next.js 15 使用 (16ではなく)**: ユーザー要望に基づき Next.js 15.5.15 を採用
2. **pnpm 使用**: ワークスペースが npm を禁止していたため pnpm に変更
3. **`--webpack` フラグ不要**: Next.js 15.5 は webpack がデフォルト、`--turbopack` がopt-in
4. **`middleware.ts` 使用 (proxy.ts ではなく)**: Next.js 15 では middleware.ts が標準
5. **全画面を実装**: 計画では別Issueとされていた画面もユーザー要望により全て実装
6. **Chakra UI v3 polymorphic `as` prop 非対応**: `<Box as={Link} href="...">` がTS型エラーになるため、`<Link>` を外側に配置するパターンに変更
7. **`@eslint/eslintrc` 追加**: eslint flat config のcompat layerに必要

#### テスト結果

| 項目 | 結果 |
|------|------|
| `pnpm typecheck` | ✅ エラーなし |
| `pnpm lint` | ✅ No ESLint warnings or errors |
| `pnpm build` | ✅ 全ページ正常ビルド |

ビルド出力:
- Static pages: `/login`, `/_not-found`
- Dynamic pages (SSR): `/repositories`, `/repositories/[repositoryId]`, BoardProject, Runs各画面
- Middleware: 34.3 kB
- First Load JS: ~120 kB (各ページ)

#### 作成ファイル一覧

**設定:**
- `boardflow/package.json`
- `boardflow/tsconfig.json`
- `boardflow/next.config.ts`
- `boardflow/eslint.config.mjs`
- `boardflow/.gitignore`
- `boardflow/.env.local.example`
- `boardflow/pnpm-lock.yaml`

**認証:**
- `src/middleware.ts`
- `src/lib/auth.ts`

**API:**
- `src/lib/api/schema.d.ts`
- `src/lib/api/server.ts`
- `src/lib/api/client.ts`

**UI:**
- `src/components/ui/provider.tsx`
- `src/components/layout/header.tsx`
- `src/components/layout/sidebar.tsx`
- `src/components/layout/app-shell.tsx`

**ページ:**
- `src/app/layout.tsx`
- `src/app/login/page.tsx`
- `src/app/(authenticated)/layout.tsx`
- `src/app/(authenticated)/repositories/page.tsx`
- `src/app/(authenticated)/repositories/[repositoryId]/page.tsx`
- `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/page.tsx`
- `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/page.tsx`
- `src/app/(authenticated)/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx`

**CI:**
- `.github/workflows/ci.yml` (frontend job 追加)

### 残リスク

- E2Eテスト (Playwright) は未実装 (別Issue)
- KiCanvas viewer統合は未実装 (将来Issue)
- openapi-typescript による自動型生成は Backend OpenAPI spec 提供後に切り替え
- `next lint` deprecated 警告 (Next.js 16 で削除予定、影響なし)
- iBOM iframe表示は artifact domain 分離後に実装
- エラーバウンダリ・loading.tsx は未実装
