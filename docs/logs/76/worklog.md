# Issue #76: OAuth state mismatch修正: APIとフロントエンドのクロスドメイン対応

## 経緯
- ユーザー要望3: APIとフロントエンドのドメインが異なる場合にOAuth state mismatchが発生
- フロントエンド (Next.js, localhost:3000) と API (Rust/Axum, localhost:8080) が別ポートで動作
- Next.js `rewrites()` で `/api/*` を API サーバーにプロキシ
- login ページは `/api/v1/auth/login` にリンク（rewrite 経由）
- API が `boardflow_oauth_state` Cookie を `SameSite=Lax` で設定
- GitHub OAuth callback URL が API ドメインに直接向いている場合、Cookie が送信されない

## ユーザー要望
- クロスドメイン環境でOAuth認証を正常に動作させたい
- `redirect_uri` パラメータでフロントエンドドメインを callback 先に指定するアプローチの検証

## 調査結果

### 2026-05-04: 外部調査 (research agent)

#### 1. Next.js rewrites の Cookie 転送挙動

- Next.js rewrites はリバースプロキシとして動作
- upstream が 302 を返した場合、Next.js は **リダイレクトをフォローせず 302 + Set-Cookie をそのままブラウザに返す**
- ブラウザは Cookie を **フロントエンドドメイン** (リクエスト元) に保存する
- 出典: serverless-nextjs/serverless-next.js#929, vercel/next.js#17325

**結論**: `/api/v1/auth/login` を rewrite 経由で呼ぶと、`boardflow_oauth_state` Cookie はフロントエンドドメインに正しく保存される。

#### 2. GitHub OAuth `redirect_uri` の挙動

- `redirect_uri` は Strongly recommended パラメータ
- 省略時は GitHub OAuth App 設定の callback URL にリダイレクト
- 指定時の検証ルール:
  - ホスト（サブドメイン除く）とポートが callback URL と完全一致
  - パスは callback URL のサブディレクトリ
- ループバック URL (127.0.0.1) の場合はポート違いも許可
- 出典: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps

**結論**: GitHub OAuth App の callback URL をフロントエンドドメインに設定し、`redirect_uri` でフロントエンドドメインを指定すれば callback もフロントエンド経由になる。

#### 3. ベストプラクティス

- IETF OAuth BCP (draft-ietf-oauth-browser-based-apps) では BFF パターンとしてフロントエンドドメイン経由を推奨
- Cookie + redirect_uri 方式が最もシンプルで既存アーキテクチャに合致
- DB ベース state 管理はオーバーエンジニアリング
- SameSite=None は CSRF 保護を弱めるため不採用

#### 提案アプローチの検証結果

**提案は正しい。採用推奨。**

## 実装計画

### 2026-05-04: Plan phase

#### 目的

- APIとフロントエンドが異なるドメイン/ポートで動作する環境で、OAuth state Cookie が callback 時に送信されるようにする
- `redirect_uri` パラメータを使い、GitHub OAuth callback をフロントエンドドメイン経由にルーティングする

#### 非目的

- DB ベースの state 管理への移行
- OAuth フロー全体のリファクタリング
- SameSite=None への変更
- token refresh フローの追加

#### 受け入れ条件

1. `login` ハンドラが生成する GitHub authorize URL に `redirect_uri={BOARDFLOW_APP_DOMAIN}/api/v1/auth/callback` が含まれること
2. `callback` ハンドラの token exchange にも同じ `redirect_uri` が渡されること
3. `BOARDFLOW_APP_DOMAIN` が `https://` で始まる場合、Cookie に `Secure` フラグが付与されること
4. 開発環境 (`http://localhost:3000`) で Cookie に `Secure` が付かないこと
5. 既存テストが全て PASS し、新テストが追加されていること
6. CSRF 防御（state 検証）が引き続き正しく機能すること

#### 詳細要件

1. **`login` ハンドラ変更**:
   - `Extension(AppDomain)` を引数に追加
   - `BOARDFLOW_APP_DOMAIN` の末尾スラッシュをトリムして `redirect_uri` を構築
   - GitHub authorize URL に `&redirect_uri=<encoded>` を追加
   - Cookie: `BOARDFLOW_APP_DOMAIN` が `https://` で始まる場合 `; Secure` を付与

2. **`callback` ハンドラ変更**:
   - `Extension(AppDomain)` を引数に追加
   - token exchange の form パラメータに `redirect_uri` を追加（OAuth spec 推奨）
   - Cookie clear にも Secure フラグ条件を適用

3. **session cookie (callback 成功時)** にも Secure フラグ条件を適用

#### 影響範囲

| ファイル | 変更内容 |
|----------|----------|
| `crates/api/src/routes/auth.rs` | `login` / `callback` ハンドラに `AppDomain` 注入、redirect_uri 追加、Secure フラグ条件付与 |
| `crates/api/tests/auth_test.rs` | `login_app()` に `AppDomain` Extension を追加、redirect_uri 検証テスト追加 |
| `docs/spec.md` (任意) | OAuth フローの説明更新 |

#### 設計方針

```rust
// login handler signature change
pub async fn login(
    Extension(oauth_config): Extension<OAuthConfig>,
    Extension(AppDomain(app_domain)): Extension<AppDomain>,
    Query(query): Query<LoginQuery>,
) -> Response {
    let state = Uuid::new_v4().to_string();
    
    // Normalize domain: trim trailing slash
    let domain = app_domain.trim_end_matches('/');
    let redirect_uri = format!("{}/api/v1/auth/callback", domain);
    
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=read:user&state={}&redirect_uri={}",
        oauth_config.client_id,
        urlencoding::encode(&state),
        urlencoding::encode(&redirect_uri),
    );

    let is_secure = domain.starts_with("https://");
    let secure_flag = if is_secure { "; Secure" } else { "" };

    let oauth_state_cookie = format!(
        "boardflow_oauth_state={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300{}",
        state, secure_flag
    );
    // ...
}
```

```rust
// callback handler - token exchange with redirect_uri
let domain = app_domain.trim_end_matches('/');
let redirect_uri = format!("{}/api/v1/auth/callback", domain);

let token_resp = client
    .post("https://github.com/login/oauth/access_token")
    .header(header::ACCEPT, "application/json")
    .form(&[
        ("client_id", oauth_config.client_id.as_str()),
        ("client_secret", oauth_config.client_secret.as_str()),
        ("code", query.code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
    ])
    .send()
    .await
```

#### テスト観点

1. **既存テスト修正**: `login_app()` ヘルパーに `Extension(AppDomain("http://localhost:3000".to_string()))` を追加
2. **新テスト: redirect_uri が URL に含まれる**: Location ヘッダーに `redirect_uri=http%3A%2F%2Flocalhost%3A3000%2Fapi%2Fv1%2Fauth%2Fcallback` が含まれることを検証
3. **新テスト: HTTPS ドメインで Secure フラグが付く**: `AppDomain("https://app.boardflow.io")` の場合、Cookie に `Secure` が含まれることを検証
4. **新テスト: HTTP ドメインで Secure フラグが付かない**: `AppDomain("http://localhost:3000")` の場合、Cookie に `Secure` が含まれないことを検証
5. **新テスト: 末尾スラッシュの正規化**: `AppDomain("http://localhost:3000/")` でも正しい redirect_uri になることを検証

#### ドキュメント更新対象

- `docs/logs/76/worklog.md` (本ファイル): 実装進行中に追記
- GitHub OAuth App 設定手順: README.md もしくは docs 内に callback URL 設定の注意書き追加（任意）

#### 実装ステップ

1. `crates/api/src/routes/auth.rs` の `login` ハンドラを修正
   - `AppDomain` Extension を追加
   - `redirect_uri` の構築と URL パラメータ追加
   - Secure フラグの条件付与
2. `crates/api/src/routes/auth.rs` の `callback` ハンドラを修正
   - `AppDomain` Extension を追加
   - token exchange に `redirect_uri` を追加
   - session cookie / clear cookie に Secure フラグ条件を適用
3. `crates/api/tests/auth_test.rs` の修正
   - `login_app()` に `AppDomain` 追加
   - 新テスト追加
4. `cargo test -p boardflow-api` で全テスト PASS を確認
5. worklog 更新

#### 実装要否

`implementation_required`

#### 未解決の疑問

なし — 調査で全て解消済み。

#### 残リスク

- GitHub OAuth App の callback URL 設定は手動変更が必要（本番デプロイ時に確認）
- `next.config.ts` の `API_BASE_URL` が正しく `localhost:8080` を指していないと rewrite が自己ループする可能性（#75 で修正済み）

---

### 作業ログパス

`docs/logs/76/worklog.md`

修正後のフロー:
```
[Browser] → GET localhost:3000/api/v1/auth/login
  ↓ (Next.js rewrite → localhost:8080)
[API] → 302 GitHub + Set-Cookie: boardflow_oauth_state=XXX
  ↓ (Next.js passthrough)
[Browser] ← 302 + Set-Cookie (Cookie は localhost:3000 に保存)
  ↓
[Browser] → GitHub authorize?...&redirect_uri=http://localhost:3000/api/v1/auth/callback
  ↓ (ユーザー認可)
[GitHub] → 302 http://localhost:3000/api/v1/auth/callback?code=...&state=XXX
  ↓
[Browser] → GET localhost:3000/api/v1/auth/callback?code=...&state=XXX
              Cookie: boardflow_oauth_state=XXX  ← 送信される
  ↓ (Next.js rewrite → localhost:8080)
[API] ← Cookie state == query state → 認証成功
```

#### 必要な実装変更

1. **`crates/api/src/routes/auth.rs` の `login` ハンドラ**:
   - `AppDomain` を注入し `redirect_uri={app_domain}/api/v1/auth/callback` を authorize URL に追加
2. **GitHub OAuth App 設定** (手動):
   - callback URL をフロントエンドドメインに変更
3. **token exchange の `redirect_uri`** (推奨):
   - `POST github.com/login/oauth/access_token` にも `redirect_uri` を渡す

#### 注意点

- `BOARDFLOW_APP_DOMAIN` の末尾スラッシュ有無に注意（URL 結合バグ）
- `redirect_uri` は URL エンコードが必要
- 本番環境では Cookie に `Secure` フラグ追加を検討
- GitHub OAuth App の callback URL も変更が必要（運用作業）

---

## 実装結果

### 2026-05-04: Implementation phase

#### 変更内容

| ファイル | 変更 |
|----------|------|
| `crates/api/src/routes/auth.rs` | `login` / `callback` / `logout` に `AppDomain` Extension 注入。`redirect_uri` を GitHub authorize URL と token exchange に追加。Cookie に条件付き `Secure` フラグ追加。 |
| `crates/api/tests/auth_test.rs` | `login_app_with_domain()` ヘルパー追加。新テスト6件追加。既存テストに `AppDomain` Extension 追加。 |

#### 追加テスト (6件)

| テスト名 | 保証する観点 |
|---------|-------------|
| `test_login_redirect_contains_redirect_uri` | Location ヘッダーに `redirect_uri` パラメータが存在する |
| `test_login_redirect_uri_uses_app_domain` | `redirect_uri` が設定した `AppDomain` に基づいて構築される |
| `test_login_redirect_uri_trims_trailing_slash` | 末尾スラッシュのある domain でも正しく正規化される |
| `test_login_https_domain_sets_secure_cookie` | HTTPS ドメインで `oauth_state` Cookie に `Secure` フラグが付く |
| `test_login_https_domain_redirect_to_cookie_has_secure` | HTTPS ドメインで `redirect_to` Cookie にも `Secure` フラグが付く |
| `test_login_http_domain_no_secure_cookie` | HTTP ドメインでは `Secure` フラグが付かない |

#### テスト結果

```
running 17 tests
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

- `config_test` の1件は環境変数依存の既存テストで本Issue無関係

#### コミット

```
fix(auth): add redirect_uri to OAuth flow and conditional Secure cookie flag
```

#### 残リスク

1. `callback` / `logout` ハンドラの統合テスト（DB + GitHub API mock）は本PRのスコープ外
2. GitHub OAuth App の callback URL 設定は手動変更が必要（本番デプロイ時）
3. `next.config.ts` の API_BASE_URL 設定が正しいことが前提（#75 で対応済み）

### 調査メモ

- `docs/external/oauth-state-cross-domain.md` に詳細を記録

## 結論ステータス

`implementation_required`

## 残リスク
- GitHub OAuth App の callback URL 設定が現在 API ドメイン向きの場合、手動変更が必要
- 開発環境（HTTP）と本番環境（HTTPS）での Cookie Secure フラグの挙動差異
- `BOARDFLOW_APP_DOMAIN` 末尾スラッシュの統一が未確認
