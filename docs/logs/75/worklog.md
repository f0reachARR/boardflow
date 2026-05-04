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

## レビュー結果（2026-05-04 review エージェント）

### 総評
- ループの主因だった `/login` への middleware リダイレクトは解消できており、`redirect_to` の伝搬方針自体も middleware → login page → backend login → callback で一貫している。
- 一方で、認証済みユーザーの `/login` 直接アクセスが未処理になっており、計画・research で前提にしていた「login ページ側での認証済みリダイレクト」が欠けている。
- 加えて、`redirect_to` を未エスケープのまま `Set-Cookie` に埋め込んでおり、Cookie 値として不正な文字を含む入力でヘッダ破損や builder `unwrap()` panic を起こし得る。現状は PR 作成前に修正が必要。

### PR可否
- `pr_ready: false`

### 指摘事項

#### major
- [boardflow/src/app/login/page.tsx](boardflow/src/app/login/page.tsx#L4) で認証済みユーザーのリダイレクトが未実装。`middleware.ts` から `/login && session` リダイレクトを外した一方、login page 側には `getCurrentUser()` による復帰導線がないため、正常なセッションを持つユーザーが [boardflow/src/middleware.ts](boardflow/src/middleware.ts#L19) を通ってそのままログイン画面を見られる。Issue のレビュー観点にある「Cookie有効+/login直接アクセス」を満たせておらず、research 文書でも `/login` ページ側が認証済みユーザーを処理する前提になっている。

修正案:
- login page で `getCurrentUser()` を呼び、認証済みなら `/repositories` もしくは検証済み `redirect_to` に `redirect()` する。
- 少なくとも既存の「ログイン済みユーザーが `/login` を見ない」挙動は維持する。

#### major
- [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L59) から [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L64) で、`redirect_to` を生文字列のまま `Set-Cookie` に埋め込んでいる。`validate_redirect_path()` は open redirect の観点では有効だが、Cookie 値として禁止される `;`, `,`, space, DQUOTE, 制御文字 CR/LF などを拒否していないため、`/api/v1/auth/login?redirect_to=/%0d%0a...` や `/%3B Secure` のような入力でヘッダ生成失敗や意図しない cookie 属性混入を招く。`Response::builder().header(...).body(...).unwrap()` のため、無効ヘッダ値は panic に直結する。

修正案:
- `redirect_to` を Cookie に保存する前に percent-encode するか、専用の Cookie builder を使って安全に serialize する。
- 併せて `validate_redirect_path()` で cookie-value 不許可文字と制御文字を拒否し、`login` handler の単体/統合テストに `;`, CR/LF, 空白, DQUOTE を追加する。

### 任意改善
- [boardflow/src/middleware.ts](boardflow/src/middleware.ts#L19) の `pathname.startsWith('/login')` は `/login-foo` も public 扱いする。現状ルート構成では直ちに問題化しないが、将来の保護ルート追加時に踏み抜きやすいので `pathname === '/login' || pathname.startsWith('/login/')` のように境界を切った方が安全。
- [boardflow/src/app/login/page.tsx](boardflow/src/app/login/page.tsx#L7) の `searchParams` 型は実行時に配列値も来得るため、`string | string[] | undefined` を扱う形の方が Next.js 15 の実態に近い。

### テスト不足
- backend 側は [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L330) の pure function テストしか増えておらず、`login`/`callback` の Cookie 往復、複数 `Set-Cookie`、不正 `redirect_to` 入力時の挙動を検証する HTTP レベルのテストがない。
- frontend 側は middleware と login page の回帰を担保するテストが見当たらない。少なくとも「無効 session で protected route → `/login?redirect_to=...`」「有効 session で `/login` → `/repositories`」の導線は E2E か integration で欲しい。

### ドキュメント確認
- research 成果物の [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L95) は今回の方針と整合している。
- ただし API 仕様書の [docs/backend/api.md](docs/backend/api.md#L355) から [docs/backend/api.md](docs/backend/api.md#L380) は旧挙動のままで、`GET /api/v1/auth/login` の `redirect_to` クエリ、`boardflow_redirect_to` Cookie、callback の可変リダイレクト先が未反映。

### plan / research / docs との不整合
- research では `/login` ページが認証済みユーザーのリダイレクトを担当すると整理されているが、実装は未対応。
- docs/backend/api.md は callback の戻り先を `/` 固定と記載しており、実装後の契約と不一致。

### 残リスク
- Cookie 値の直列化不備を残したままでは、不正クエリによる login handler の異常系が未管理のまま残る。
- `/login` の認証済みリダイレクト未実装のままだと、今後 login 画面に説明や副作用を足した際に意図しない再認証導線を開き続ける。

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

---

## レビュー結果（2026-05-04 review エージェント 再レビュー）

### 総評
- 前回の major 指摘 2 件はコード上いずれも解消されている。`/login` は middleware で常時通過になり、[boardflow/src/app/login/page.tsx](boardflow/src/app/login/page.tsx#L1) で認証済みユーザーを `/repositories` へ戻すため、無効 session のループ回避と有効 session の `/login` 滞留回避は両立できている。
- [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L295) の `validate_redirect_path()` には cookie unsafe 文字拒否が追加され、少なくとも今回前回指摘していた `Set-Cookie` 破損要因は抑えられている。
- ただし、auth フローの契約変更に対する API ドキュメント更新と HTTP レベルの回帰テストが未整備で、この Issue の完了条件としてはまだ弱い。

### PR可否
- `pr_ready: false`

### 指摘事項

#### medium
- [docs/backend/api.md](docs/backend/api.md#L350) から [docs/backend/api.md](docs/backend/api.md#L380) が実装と一致していない。現行実装では `GET /api/v1/auth/login` が `redirect_to` を受け付け、[crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L20) から [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L58) で `boardflow_redirect_to` cookie を保存し、callback も [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L161) から [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L182) で `/` 固定ではなく検証済みの相対パスへ戻す。仕様書が旧契約のままだと、後続実装と運用判断を誤らせる。

#### medium
- auth 変更の検証が pure function テストに偏っている。[crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L321) 以降のユニットテストは `validate_redirect_path()` のみで、[crates/api/tests/auth_test.rs](crates/api/tests/auth_test.rs#L1) には `login` と `callback` の cookie 受け渡し、複数 `Set-Cookie`、不正 `redirect_to` 入力時のフォールバックを確認するテストがない。今回の修正は認証導線そのものに触れているため、HTTP レベルで 1 本でも回帰テストが欲しい。

### 必須修正
- `docs/backend/api.md` を実装後の認証フローに合わせて更新する。
- `crates/api/tests/auth_test.rs` などで `redirect_to` を含む login/callback フローの統合テストを追加する。

### 任意改善
- [boardflow/src/app/login/page.tsx](boardflow/src/app/login/page.tsx#L6) の認証済みユーザーリダイレクトは常に `/repositories` 固定だが、`redirect_to` が安全な相対パスならそれを優先しても UX は自然になる。

### テスト結果
- `mise exec -- cargo test -p boardflow-api routes::auth -- --nocapture` : 10 テスト成功
- `pnpm --dir /home/f0reach/workspace/boardflow/boardflow build` : 成功

### ドキュメント確認
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L1) との整合は取れている。
- [docs/backend/api.md](docs/backend/api.md#L350) から [docs/backend/api.md](docs/backend/api.md#L380) は未更新。

### plan / research / docs との不整合
- research / plan は `redirect_to` の伝搬と callback 側の可変リダイレクトを前提にしているが、API 仕様書だけが `/` 固定のまま残っている。

### 残リスク
- 文書化されないまま merge すると、今後の auth 実装修正時に旧仕様を前提に戻されるリスクがある。
- HTTP レベルの回帰テストがないため、cookie の直列化や複数 `Set-Cookie` の将来リグレッションを検知しづらい。

---

## レビュー結果（2026-05-04 review エージェント 最終再レビュー）

### 総評
- Issue #75 の目的である「無効セッション時の `/login` ループ解消」と「ログイン後の元ページ復帰」は、[boardflow/src/middleware.ts](boardflow/src/middleware.ts#L1)・[boardflow/src/app/login/page.tsx](boardflow/src/app/login/page.tsx#L1)・[crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L1) の実装で一貫して満たせている。
- 前回レビューで必須だった API 仕様書更新と login handler の HTTP レベル統合テスト追加も反映済みで、実装・research・plan・ドキュメントの不整合は解消された。
- 今回確認できた範囲では、PR 作成を止める欠陥は見当たらない。

### PR可否
- `pr_ready: true`

### 指摘事項
- blocking / major / medium の指摘事項なし。

### 必須修正
- なし。

### 任意改善
- [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L82) 以降の callback 分岐は今回変更範囲だが、現状は専用の HTTP レベル回帰テストがない。将来の回帰検知を強めるなら、`boardflow_redirect_to` cookie を与えた callback のフォールバックと cookie 削除を確認するテストを追加するとよい。

### テスト不足
- 非ブロッキングではあるが、auth 統合テストは login handler 側に寄っており、callback 側の `redirect_to` 利用と cookie clear の end-to-end 検証は未追加。

### テスト結果
- `mise exec -- cargo test -p boardflow-api --test auth_test` : 11 テスト成功
- `pnpm --dir /home/f0reach/workspace/boardflow/boardflow build` : 成功

### ドキュメント確認
- [docs/backend/api.md](docs/backend/api.md#L355) から [docs/backend/api.md](docs/backend/api.md#L394) に `redirect_to` query、`boardflow_redirect_to` cookie、callback の可変リダイレクトと cookie 削除が反映されている。
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L1) の調査内容と現在の実装方針は整合している。

### plan / research / docs との不整合
- 確認した範囲では不整合なし。

### PR/完了結果
- review 判定: `pr_ready: true`
- review フェーズ完了。Issue #75 は PR 作成へ進めてよい。

### 残リスク
- callback 分岐に専用の統合テストがないため、将来の認証フロー変更時に cookie 削除やリダイレクト先フォールバックの回帰を見逃す可能性がある。

---

## PR/完了結果（2026-05-04 pr エージェント）

### PR作成

- **PR URL**: https://github.com/f0reachARR/boardflow/pull/80
- **ブランチ**: `fix/75-auth-redirect-loop` → `main`
- **コミット**:
  - `82c03ab` — fix(auth): redirect loop fix & post-login redirect to original page (#75)
  - `2fab618` — docs: update worklog for #75 with implementation results
  - `1f2ea33` — fix(auth): apply review fixes - authenticated user redirect, boundary check, integration tests, OpenAPI params

### 最終確認

- review: `pr_ready: true`（blocking指摘なし）
- docs: `docs_ready: true`（実装・仕様書・生成契約の整合確認済み）
- 未コミット変更なし（PR関連ファイル全てコミット済み）
- テスト: Rust 11テスト全パス、Next.js build成功

### 残リスク

1. callback 分岐の HTTP レベル統合テスト未追加（非ブロッキング）
2. 複数タブの `redirect_to` Cookie競合（実用上許容）
3. `x-middleware-cache: no-cache` は community workaround（公式 guarantee なし）

---

## ドキュメント確認結果（2026-05-04 docs エージェント）

### 対象Issue
- Issue #75: フロントエンド認証リダイレクトループ修正 & ログイン後の元ページリダイレクト

### 総評
- [docs/backend/api.md](docs/backend/api.md#L355) から [docs/backend/api.md](docs/backend/api.md#L394) の Auth API 記述は、今回の実装である [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L24) から [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L211) と整合している。
- [docs/spec.md](docs/spec.md#L1619) と [docs/frontend/summary.md](docs/frontend/summary.md#L77) の認証記述は高レベル方針に留まっており、今回の `redirect_to` 追加や `/login` ループ回避の詳細を反映する必然性はない。
- 一方で、生成済み契約ファイル [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L6) と [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L22) では `/api/v1/auth/callback` と `/api/v1/auth/login` の query parameter が `never` のままで、実装および API 仕様書と不一致になっている。PR を出す前に OpenAPI 側の注釈または生成物更新が必要。

### docs_ready
- `docs_ready: false`

### 必須修正
- OpenAPI 契約を更新し、[boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L6) と [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L22) に auth query parameter が反映される状態にする。
  - 現状は `GET /api/v1/auth/login` の `redirect_to` と `GET /api/v1/auth/callback` の `code`, `state` が生成型に現れていない。
  - ドキュメント本文は更新済みでも、参照される API 契約面が旧仕様のままだと後続実装とレビューで齟齬が残る。

### 任意改善
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L147) の `x-middleware-cache: no-cache` については、現行の Next.js 公式 API ドキュメントでは明示的な保証を確認できなかった。コミュニティ workaround としては妥当だが、「推奨」や「防げる」と断定するより「報告例がある workaround」と明記した方が根拠の強さに見合う。

### 不整合のあるドキュメント
- なし。`docs/` 配下で今回確認対象にした文書のうち、実装と明確に矛盾するものは見当たらない。

### 不足しているドキュメント
- 追加の README / CONTRIBUTING 更新は不要。
- ただし生成契約ファイル [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts) は更新漏れの可能性が高い。

### 外部調査メモに関する指摘
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L11) の「`NextResponse.redirect()` で生成した response に対して `.cookies.delete()` を呼べる」は、現行の Next.js `NextResponse` 公式 docs と整合している。
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L69) の「Server Component では `cookies().delete()` が使えず、読み取り専用」は、現行の Next.js `cookies` 公式 docs と整合している。
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L147) の `x-middleware-cache` に関する記述は、公式 docs ではなく issue / community report ベースのため、根拠レベルを一段下げて表現するのが安全。

### PR/完了結果
- docs review 判定: `docs_ready: false`
- blocking 理由: `docs/` 本文は整合しているが、生成済み API 契約が旧仕様のままで参照面に齟齬が残るため。

### 残リスク
- OpenAPI 生成物を未更新のまま merge すると、frontend 側の将来の API 利用やレビューが旧 auth 契約を前提に進む可能性がある。
- `x-middleware-cache` の記述を強い断定のまま残すと、Next.js 側の挙動保証と誤認される可能性がある。

---

## ドキュメント確認結果（2026-05-04 docs エージェント 再確認）

### 対象Issue
- Issue #75: フロントエンド認証リダイレクトループ修正 & ログイン後の元ページリダイレクト

### 総評
- 前回 blocking としていた OpenAPI 注釈不足は、[crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L22) と [crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L68) で `IntoParams` と `params(...)` が追加されており、ソース側では解消されている。
- [docs/backend/api.md](docs/backend/api.md#L355) の Auth API 記述と、[boardflow/src/middleware.ts](boardflow/src/middleware.ts#L1)、[boardflow/src/app/login/page.tsx](boardflow/src/app/login/page.tsx#L1)、[crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L24) 以降の実装は整合している。
- ただし、現時点の生成済み契約ファイル [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L6) と [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L22) は依然として query parameter が `never` のままで、リポジトリ上の参照契約は旧仕様のまま残っている。現ブランチをそのまま PR 化する前提では docs review はまだ通せない。

### docs_ready
- `docs_ready: false`

### 必須修正
- [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts) を、追加済みの utoipa 注釈が反映された状態に再生成する。
- もし CI または merge 後生成を前提にする運用なら、その前提を PR 本文に明記し、このブランチでは生成物差分が未反映であることをレビューアに共有する。

### 任意改善
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L147) の `x-middleware-cache: no-cache` は公式 guarantee ではなく報告ベースの workaround と分かる表現に寄せると、根拠の強さと文章が揃う。

### 不整合のあるドキュメント
- [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts): 実装と API 仕様書では auth query parameter が存在するが、生成契約では未反映。

### 不足しているドキュメント
- README / CONTRIBUTING の追加更新は不要。
- ただし、生成契約の更新が未反映なため、利用者が参照する API 面の最新化が不足している。

### 外部調査メモに関する指摘
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L1) の採用判断自体は現在の実装方針と矛盾していない。
- `x-middleware-cache` の説明だけは、公式 docs よりコミュニティ報告への依存が強い点を弱めに表現した方が安全。

### PR/完了結果
- docs review 判定: `docs_ready: false`
- blocking 理由: ソース注釈は修正済みだが、現ブランチの生成契約が旧仕様のままで参照面に齟齬が残るため。

### 残リスク
- 生成物未更新のまま merge されると、frontend の型生成や後続レビューで auth query parameter が存在しない前提が残る。

---

## ドキュメント確認結果（2026-05-04 docs エージェント 最終確認）

### 対象Issue
- Issue #75: フロントエンド認証リダイレクトループ修正 & ログイン後の元ページリダイレクト

### 総評
- [docs/backend/api.md](docs/backend/api.md#L355) から [docs/backend/api.md](docs/backend/api.md#L394) の Auth API 記述は、[crates/api/src/routes/auth.rs](crates/api/src/routes/auth.rs#L22) 以降の実装と整合している。
- 前回 blocking としていた [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts) の query parameter 未反映は、生成ファイル先頭の `paths[...].parameters.query` ではなく `operations.login.parameters.query` / `operations.callback.parameters.query` を参照すべき生成形式の読み違いだった。現行ファイルでは [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L768) から [boardflow/src/lib/api/schema.d.ts](boardflow/src/lib/api/schema.d.ts#L813) に `redirect_to` / `code` / `state` が反映されており、`openapi-fetch` の利用形とも整合する。
- 今回確認対象の docs / docs/external / 生成契約の範囲では、PR 作成を止めるドキュメント不整合は見当たらない。

### docs_ready
- `docs_ready: true`

### 必須修正
- なし。

### 任意改善
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L147) の `x-middleware-cache: no-cache` は、公式 guarantee というより報告ベースの workaround と分かる表現に寄せると根拠の強さと記述が揃う。

### 不整合のあるドキュメント
- なし。

### 不足しているドキュメント
- なし。README / CONTRIBUTING への今回追記も不要。

### 外部調査メモに関する指摘
- [docs/external/nextjs-middleware-cookie-redirect-loop.md](docs/external/nextjs-middleware-cookie-redirect-loop.md#L1) の採用判断は、今回の middleware / login page / backend auth の実装方針と矛盾していない。
- `x-middleware-cache` に関する記述だけは、根拠レベルに合わせて表現を少し弱める余地があるが、Issue #75 の blocking ではない。

### テスト結果
- ユーザー共有の結果として Rust テストは全通、Next.js build は成功。
- 端末履歴でも `pnpm build` 成功は確認できる。

### PR/完了結果
- docs review 判定: `docs_ready: true`
- docs フェーズ完了。Issue #75 は PR 作成へ進めてよい。

### 残リスク
- callback 側の `redirect_to` cookie 利用については、将来の回帰検知をさらに強めるなら HTTP レベル統合テストがあるとより堅い。
