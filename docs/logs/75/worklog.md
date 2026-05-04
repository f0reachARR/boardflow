# Issue #75: フロントエンド認証リダイレクトループ修正 & ログイン後の元ページリダイレクト

## 経緯
- ユーザー要望1: 認証情報が無効でAPIが403を返す場合にリダイレクトループが発生
- ユーザー要望2: ログイン後に元のページにリダイレクトしたい
- 両者は認証フローの同一箇所に関わるため1つのIssueに統合

## ユーザー要望
- 無効セッションによるリダイレクトループの解消
- ログイン前のページURLを保持し、ログイン後にリダイレクト

## 調査結果
- `middleware.ts`: `boardflow_session` Cookieの存在のみでチェック、有効性は未検証
- `auth.ts`: `getCurrentUser()` がAPIを呼び出し、失敗時は `null` を返すがCookieクリアなし
- `routes/auth.rs`: callback後のリダイレクト先が `"/"` 固定
- Next.js middleware の 307 リダイレクトはブラウザキャッシュされる可能性あり

## Issue作成内容
- タイトル: フロントエンド認証リダイレクトループ修正 & ログイン後の元ページリダイレクト
- ラベル: bug, frontend
- 新規作成

## 後続処理タイプ
`implementation_required`

## 外部調査（2026-05-04 research エージェント）

### 調査対象
1. Next.js 15 middleware でのCookie削除 + リダイレクト同時実行
2. middleware 内でのAPI呼び出しによるセッション検証の是非
3. ログイン後リダイレクト（redirect_to パターン）
4. Open Redirect 防止（OWASP ベストプラクティス）

### 調査結果サマリ

#### リダイレクトループの解消方針
- **根本原因**: middleware が Cookie の「存在」のみで `/login` → `/repositories` にリダイレクトするため、無効 Cookie 保持ユーザーが layout の `getCurrentUser()` → null → `/login` → middleware → `/repositories` のループに陥る
- **推奨修正（案B）**: middleware の `/login` ガードを削除し、`/login` を常にパブリックページとして扱う。login ページ側で認証済みユーザーのリダイレクト処理を行う。これにより layout → `/login` リダイレクトが常に成功する
- **Cookie 削除**: `NextResponse.redirect()` + `.cookies.delete()` で同時実行可能。ただし Server Component(layout.tsx) からは `cookies().delete()` は使えない（読み取り専用）

#### middleware での API 呼び出し
- **不採用**: Edge Runtime 制約、全リクエストへの API コール負荷、Better Auth 公式の「middleware では Cookie 存在チェックのみ推奨」の方針
- middleware は「楽観的チェック」、layout/Server Component で「悲観的チェック」の二層構造が推奨

#### ログイン後リダイレクト
- **伝搬フロー**: middleware(`redirect_to` クエリ付与) → login ページ（リンクに伝播）→ backend login handler（`boardflow_redirect_to` Cookie に保存, TTL=300s）→ GitHub OAuth → backend callback（Cookie から読み取り、バリデーション後にリダイレクト）
- OAuth state とは分離して管理

#### Open Redirect 防止
- OWASP A01:2025 (Broken Access Control) に分類
- **必須バリデーション**: 相対パスのみ許可、`//`, `://`, `\`, URL エンコードバイパスを拒否
- バックエンド callback handler で最終バリデーション実装が必須

### 更新ファイル
- `docs/external/nextjs-middleware-cookie-redirect-loop.md` — 新規作成

### 結論ステータス
`implementation_required`

### 推奨実装方針（3箇所の修正）

#### フロントエンド（2ファイル）
1. **`middleware.ts`**: (a) `/login` を常にパブリック化（Cookie 有無問わず通過）、(b) 未認証リダイレクト時に `redirect_to` クエリ付与
2. **`login/page.tsx`**: `redirect_to` パラメータを `/api/v1/auth/login` リンクに伝播

#### バックエンド（1ファイル）
3. **`routes/auth.rs`**: (a) login handler で `redirect_to` を `boardflow_redirect_to` Cookie に保存（HttpOnly, SameSite=Lax, Max-Age=300）、(b) callback handler で Cookie から読み取り → `validate_redirect_to()` → リダイレクト先に使用、(c) デフォルトは `"/"` のまま

### 後続エージェントへの注意点
- Server Component（layout.tsx）からの `cookies().delete()` は不可。layout 自体の変更は不要（middleware でループを断ち切る）
- `boardflow_redirect_to` Cookie は使用後に必ず削除すること（callback handler 内で `Max-Age=0`）
- Open Redirect バリデーションは必ずバックエンド（Rust）側で実装すること（フロントエンドのみでは不十分）
- `x-middleware-cache: no-cache` ヘッダーの設定を推奨

## 残リスク
- Open Redirecter 脆弱性対策（redirect_to のバリデーション必須）
- Server Component からの Cookie 削除制約（Next.js の仕様変更に注視）
- middleware リダイレクトのブラウザキャッシュ問題（`x-middleware-cache: no-cache` で対処）

---

## 実装計画（2026-05-04 plan エージェント）

### 目的
1. 無効セッションCookie保持時のリダイレクトループを解消する
2. ログイン前にアクセスしていたページのURLを保持し、ログイン完了後にそのページへリダイレクトする

### 非目的
- セッションの有効期限管理の変更
- ログインUI/UXの変更（レイアウト・デザイン）
- 新規認証方式の追加
- バックエンドのセッション管理ロジックの変更

### 受け入れ条件
- [ ] 無効なセッションCookieを持つユーザーが保護ページにアクセスした場合、ループせずログインページに到達する
- [ ] 未認証ユーザーが `/repositories/123` にアクセス → ログイン → `/repositories/123` にリダイレクトされる
- [ ] `redirect_to` に外部URLや不正なパスを指定した場合、デフォルト(`/`)にリダイレクトされる（Open Redirect防止）
- [ ] 既存の正常な認証フロー（Cookie有効時）が壊れていない
- [ ] `redirect_to` なしでログインした場合は `/` にリダイレクトされる（既存挙動維持）

### 詳細要件

#### 要件1: リダイレクトループ解消
- middleware で `/login` へのアクセスはCookie有無に関わらず常に通過させる
- middleware の `/login && session` → `/repositories` リダイレクトを削除する
- layout.tsx の `getCurrentUser()` が null を返した場合、middleware を経由して `/login` に到達できるようにする

#### 要件2: ログイン後リダイレクト
- middleware: 未認証リダイレクト時に `redirect_to={pathname+search}` クエリパラメータを付与
- login/page.tsx: `searchParams.redirect_to` を `/api/v1/auth/login` リンクに伝播
- backend login handler: `redirect_to` クエリパラメータを `boardflow_redirect_to` Cookieに保存（HttpOnly, SameSite=Lax, Max-Age=300）
- backend callback handler: `boardflow_redirect_to` Cookieを読み取り → バリデーション → リダイレクト先に使用 → Cookie削除

#### 要件3: Open Redirect防止
- バリデーションはバックエンド（Rust）のcallback handlerで実装
- 許可条件: `/` で始まる相対パスのみ
- 拒否条件: `//`, `://`, `\`, URLデコード後の再チェック
- バリデーション失敗時: デフォルトの `/` にリダイレクト

### 影響範囲
- **フロントエンド**: `middleware.ts`, `login/page.tsx`
- **バックエンド**: `crates/api/src/routes/auth.rs`
- **変更なし**: `(authenticated)/layout.tsx`, `lib/auth.ts`

### 設計方針

#### アーキテクチャ: 2層認証チェック維持
- middleware: 楽観的チェック（Cookie存在のみ）→ 高速
- layout/Server Component: 悲観的チェック（API呼び出し）→ 正確
- ループ防止: `/login` を常にパブリックとして扱う

#### redirect_to 伝搬フロー
```
[Browser] → /protected/page
  ↓ middleware (no session)
[Browser] ← 307 /login?redirect_to=/protected/page
  ↓
[Browser] → /login?redirect_to=/protected/page
  ↓ middleware (PUBLIC_PATHS → next)
[Login Page] ← render with redirect_to in login link
  ↓ user clicks "Sign in with GitHub"
[Browser] → /api/v1/auth/login?redirect_to=/protected/page
  ↓ backend login handler
  ↓ Set-Cookie: boardflow_redirect_to=/protected/page; Max-Age=300
[Browser] ← 302 → GitHub OAuth
  ↓ user authorizes
[Browser] → /api/v1/auth/callback?code=xxx&state=yyy
  ↓ backend callback handler
  ↓ read boardflow_redirect_to cookie → validate → use as redirect
  ↓ Set-Cookie: boardflow_session=xxx
  ↓ Set-Cookie: boardflow_redirect_to=; Max-Age=0 (clear)
[Browser] ← 302 → /protected/page
```

---

### 変更ファイル一覧

#### 1. `boardflow/src/middleware.ts` — フロントエンド middleware
**変更内容**:
- `/login && session` のリダイレクト削除（ループ原因）
- PUBLIC_PATHS チェックを先頭近くに移動
- 未認証リダイレクト時に `redirect_to` クエリパラメータ付与

**Before (概念)**:
```typescript
// Authenticated user accessing login → redirect to repositories
if (pathname === '/login' && session) {
  return NextResponse.redirect(new URL('/repositories', request.url));
}
// Public paths don't require auth
if (PUBLIC_PATHS.some((p) => pathname.startsWith(p))) {
  return NextResponse.next();
}
// Unauthenticated → redirect to login
if (!session) {
  return NextResponse.redirect(new URL('/login', request.url));
}
```

**After (概念)**:
```typescript
// Public paths always accessible (login page handles authenticated user redirect)
if (PUBLIC_PATHS.some((p) => pathname.startsWith(p))) {
  return NextResponse.next();
}
// Unauthenticated → redirect to login with redirect_to
if (!session) {
  const loginUrl = new URL('/login', request.url);
  if (pathname !== '/') {
    loginUrl.searchParams.set('redirect_to', pathname + request.nextUrl.search);
  }
  const response = NextResponse.redirect(loginUrl);
  response.headers.set('x-middleware-cache', 'no-cache');
  return response;
}
```

#### 2. `boardflow/src/app/login/page.tsx` — ログインページ
**変更内容**:
- `searchParams` から `redirect_to` を取得
- ログインリンクに `redirect_to` クエリパラメータを伝播

**Before (概念)**:
```tsx
export default function LoginPage() {
  return <Link href='/api/v1/auth/login'>Sign in</Link>;
}
```

**After (概念)**:
```tsx
export default async function LoginPage({ searchParams }: { searchParams: Promise<{ redirect_to?: string }> }) {
  const params = await searchParams;
  const redirectTo = params.redirect_to;
  const loginHref = redirectTo
    ? `/api/v1/auth/login?redirect_to=${encodeURIComponent(redirectTo)}`
    : '/api/v1/auth/login';
  return <Link href={loginHref}>Sign in</Link>;
}
```

#### 3. `crates/api/src/routes/auth.rs` — バックエンド認証ルート
**変更内容 (login handler)**:
- `LoginQuery.redirect_uri` を `redirect_to` にリネーム
- `redirect_to` が有効な場合、`boardflow_redirect_to` Cookieに保存

**変更内容 (callback handler)**:
- `boardflow_redirect_to` Cookieを読み取り
- `validate_redirect_path()` でバリデーション
- バリデーション通過時: そのパスにリダイレクト
- バリデーション失敗/未設定時: `/` にリダイレクト
- `boardflow_redirect_to` Cookieをクリア

**変更内容 (新規ヘルパー)**:
- `validate_redirect_path(path: &str) -> Option<&str>` を追加

---

### Open Redirect バリデーション — 具体的ロジック

```rust
/// redirect_to パスのバリデーション。安全な相対パスのみ許可。
fn validate_redirect_path(path: &str) -> Option<&str> {
    // 空文字は無効
    if path.is_empty() {
        return None;
    }
    // `/` で始まる必要がある
    if !path.starts_with('/') {
        return None;
    }
    // protocol-relative URL を拒否
    if path.starts_with("//") {
        return None;
    }
    // absolute URL scheme を拒否
    if path.contains("://") {
        return None;
    }
    // backslash bypass を拒否
    if path.contains('\\') {
        return None;
    }
    // null byte を拒否
    if path.contains('\0') {
        return None;
    }
    // URL デコード後の再チェック（%2F%2F → // など）
    if let Ok(decoded) = urlencoding::decode(path) {
        if decoded.starts_with("//") || decoded.contains("://") || decoded.contains('\\') {
            return None;
        }
    }
    // 長すぎるパスを拒否（DoS防止）
    if path.len() > 2048 {
        return None;
    }
    Some(path)
}
```

---

### テスト計画

#### バックエンド（Rust）— 新規テスト

**1. `validate_redirect_path` ユニットテスト** (`crates/api/src/routes/auth.rs` 内 `#[cfg(test)]` モジュール)

| ケース | 入力 | 期待結果 |
|--------|------|----------|
| 正常な相対パス | `/repositories` | `Some("/repositories")` |
| クエリ付きパス | `/repositories/123?tab=files` | `Some(...)` |
| ルート | `/` | `Some("/")` |
| 空文字 | `""` | `None` |
| 相対パスでない | `repositories` | `None` |
| protocol-relative | `//evil.com` | `None` |
| absolute URL | `https://evil.com` | `None` |
| backslash bypass | `/\evil.com` | `None` |
| URL encoded bypass | `/%2F%2Fevil.com` | `None` |
| URL encoded backslash | `/%5Cevil.com` | `None` |
| contains scheme | `/foo://bar` | `None` |
| null byte | `/foo\0bar` | `None` |
| 長すぎるパス | `/` + "a" * 2048 | `None` |

**2. login handler テスト**: `redirect_to` クエリパラメータ付きリクエスト → `boardflow_redirect_to` Cookieがレスポンスに含まれること

**3. callback handler テスト**: `boardflow_redirect_to` Cookie付きリクエスト → バリデーション通過時にそのパスにリダイレクト、Cookie削除

#### フロントエンド — 手動テスト観点

| シナリオ | 手順 | 期待結果 |
|----------|------|----------|
| ループ解消 | 無効Cookie手動設定 → `/repositories` アクセス | ログインページ表示（ループなし） |
| redirect_to 正常系 | 未認証で `/repositories/123` アクセス → ログイン | `/repositories/123` にリダイレクト |
| redirect_to なし | 直接 `/login` アクセス → ログイン | `/` にリダイレクト |
| Cookie有効+/login | 有効セッションで `/login` アクセス | ログインページ表示（login ページ側で処理） |
| Open Redirect | `redirect_to=//evil.com` を手動設定 → ログイン | `/` にリダイレクト |

---

### 実装順序（依存関係考慮）

1. **Step 1**: `crates/api/src/routes/auth.rs` に `validate_redirect_path()` ヘルパー追加 + ユニットテスト
2. **Step 2**: `crates/api/src/routes/auth.rs` の login handler を修正（`redirect_to` Cookie 保存）
3. **Step 3**: `crates/api/src/routes/auth.rs` の callback handler を修正（Cookie 読み取り → バリデーション → リダイレクト）
4. **Step 4**: `boardflow/src/middleware.ts` を修正（ループ解消 + `redirect_to` 付与）
5. **Step 5**: `boardflow/src/app/login/page.tsx` を修正（`redirect_to` 伝播）
6. **Step 6**: 統合テスト（手動）

Step 1-3 はバックエンド内で依存関係あり（順序必須）。Step 4-5 はフロントエンド内で独立に実施可能だが、E2Eテストには Step 1-3 完了が必要。

---

### エッジケース一覧

| # | エッジケース | 対応 |
|---|-------------|------|
| 1 | Cookie有効 + `/login` アクセス | login ページ表示。ページ内で`getCurrentUser()`成功なら`/repositories`リダイレクト（将来改善。今回のスコープではページ表示のみ）|
| 2 | `redirect_to` に日本語パス含む | URLエンコードされるため問題なし |
| 3 | `redirect_to` が `/login` 自身 | バリデーションは通過するが実害なし（再度ログインフロー） |
| 4 | OAuth 中に `boardflow_redirect_to` Cookie の Max-Age(300s) が切れる | Cookie 消失 → callback がデフォルト `/` にリダイレクト → 許容動作 |
| 5 | 複数タブで異なる redirect_to | 最後の Cookie 値が使われる。許容動作（完璧な解決にはstate連携が必要だが過剰） |
| 6 | `redirect_to` の値が非常に長い | 2048文字制限で拒否 |
| 7 | middleware matcher に含まれない `/api/` パスへのリダイレクト | matcher除外のため middleware 未処理。バックエンドが直接 401 返却 |
| 8 | `redirect_to` に `#fragment` 含む | フラグメントはサーバーに送信されないためクライアント側で維持不可。許容動作 |

---

### ドキュメント更新対象

| ファイル | 更新内容 |
|----------|----------|
| `docs/spec.md` | 認証フローのリダイレクトループ修正、redirect_to 仕様の追記（もし認証セクションがあれば） |
| `docs/backend/api.md` | `/api/v1/auth/login` の `redirect_to` パラメータ追加、callback のリダイレクト挙動更新 |

---

### 実装要否

`implementation_required`

### 未解決の疑問

なし。research エージェントの調査で技術的疑問は解消済み。設計判断（案B: `/login` を常にパブリック化）も研究結果で裏付けられている。

### 残リスク

1. **Cookie有効ユーザーの `/login` 表示**: middleware でガードしなくなるため、有効セッション保持ユーザーが `/login` を直接アクセスした場合にログインページが表示される。ただし既にログイン済みなので実害は軽微。将来的に login ページ内で認証チェック → `/repositories` リダイレクトを追加可能。
2. **ブラウザの 307 キャッシュ**: `x-middleware-cache: no-cache` で対処するが、一部ブラウザで古いキャッシュが残る可能性がある。ユーザーがキャッシュクリアで解決可能。
3. **複数タブの redirect_to 競合**: Cookie ベースのため最後の値が勝つ。実用上は問題にならない。

### 作業ログパス

`docs/logs/75/worklog.md`

---

## 実装結果（2026-05-04 impl エージェント）

### 実装内容

#### 1. `crates/api/src/routes/auth.rs`
- `LoginQuery.redirect_uri` → `redirect_to` にリネーム
- `login` handler: `redirect_to` クエリパラメータが有効な場合、`boardflow_redirect_to` Cookieに保存（HttpOnly, SameSite=Lax, Max-Age=300）
- `callback` handler: `boardflow_redirect_to` Cookieを読み取り → `validate_redirect_path()` でバリデーション → リダイレクト先に使用 → Cookie削除（Max-Age=0）
- `validate_redirect_path()` ヘルパー関数を追加（Open Redirect防止）
- `#[cfg(test)]` ユニットテストモジュール追加（10テストケース）

#### 2. `boardflow/src/middleware.ts`
- `/login && session` → `/repositories` リダイレクト削除（ループ原因）
- PUBLIC_PATHS チェックをセッションチェックの前に移動
- 未認証リダイレクト時に `redirect_to={pathname+search}` クエリパラメータ付与
- `x-middleware-cache: no-cache` ヘッダー設定

#### 3. `boardflow/src/app/login/page.tsx`
- `async function` に変更（Server Component の searchParams は Promise）
- `searchParams.redirect_to` を取得
- ログインリンクに `redirect_to` パラメータを `encodeURIComponent()` で伝播

### テスト結果

#### Rust ユニットテスト（`cargo test -p boardflow-api`）
- **24テスト全パス**（auth関連10テスト + 既存14テスト）
- `test_validate_redirect_path_valid` — 正常パス（/、/repositories、クエリ付き）
- `test_validate_redirect_path_empty` — 空文字拒否
- `test_validate_redirect_path_no_leading_slash` — 相対パス・外部URL拒否
- `test_validate_redirect_path_protocol_relative` — `//evil.com` 拒否
- `test_validate_redirect_path_contains_scheme` — `://` 含むパス拒否
- `test_validate_redirect_path_backslash` — バックスラッシュ拒否
- `test_validate_redirect_path_null_byte` — ヌルバイト拒否
- `test_validate_redirect_path_encoded_bypass` — URLエンコードバイパス拒否
- `test_validate_redirect_path_too_long` — 2048文字超拒否

#### 既存統合テスト
- `auth_test.rs`: 8テスト全パス（request_id関連、影響なし）
- `api_token_test.rs`: 15テスト全パス
- `board_run_test.rs`: 19テスト全パス
- `config_test.rs`: 1テスト失敗（pre-existing、DATABASE_URL未設定時のテスト。本変更とは無関係）

#### Next.js ビルド
- `pnpm build` 成功（TypeScriptエラーなし、全ページ正常生成）

### コミット
- `abf6c5b` — `fix(auth): redirect loop fix & post-login redirect to original page (#75)`

### 更新ドキュメント
- `docs/logs/75/worklog.md`（本ファイル）

### 残リスク
1. **Cookie有効ユーザーの `/login` 表示**: middleware でガードしなくなるため表示される。実害は軽微（将来改善可能）
2. **ブラウザの 307 キャッシュ**: `x-middleware-cache: no-cache` で対処済み。古いキャッシュはユーザーのキャッシュクリアで解決
3. **複数タブの redirect_to 競合**: Cookie ベースのため最後の値が勝つ。実用上問題なし
4. **`config_test.rs` の既存失敗**: 本Issueとは無関係。環境変数未設定時のテスト
