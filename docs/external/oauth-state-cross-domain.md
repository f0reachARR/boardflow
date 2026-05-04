# OAuth State管理: クロスドメイン環境での Cookie ベース CSRF 防御

## 要約

APIとフロントエンドが異なるドメイン/ポートで動作する場合、OAuth state Cookie がコールバック時に送信されず state mismatch エラーが発生する。根本原因は GitHub OAuth の callback が API ドメインに直接リダイレクトされ、フロントエンドドメインで設定された Cookie が送信されないこと。解決策は `redirect_uri` パラメータでフロントエンドドメインを指定し、すべてのフローを Next.js rewrite プロキシ経由にすること。

## 確認した情報

### 1. Next.js rewrites の Cookie 転送挙動

- Next.js の `rewrites()` はリバースプロキシとして動作する
- upstream サーバーが 302 レスポンスを返した場合、**Next.js はリダイレクトをフォローせず、302 レスポンスをそのままブラウザに返す**
  - 出典: serverless-nextjs/serverless-next.js#929 で確認
  - 「The 302 response is reaching the browser with all the headers (cookies especially) and the redirect is performed client side.」
- upstream の `Set-Cookie` ヘッダーもブラウザに転送される
- ブラウザは Cookie を**リクエスト元のドメイン**（= フロントエンドドメイン）に対して保存する

**BoardFlow での挙動**:
1. ブラウザが `localhost:3000/api/v1/auth/login` にアクセス
2. Next.js rewrite が `localhost:8080/api/v1/auth/login` にプロキシ
3. API が `302 → GitHub` + `Set-Cookie: boardflow_oauth_state=...` を返す
4. Next.js がこのレスポンスをそのままブラウザに返す
5. **ブラウザは Cookie を `localhost:3000` に保存** ← ここが重要
6. ブラウザが GitHub にリダイレクト

### 2. GitHub OAuth `redirect_uri` の挙動

GitHub 公式ドキュメント (docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps) より:

- `redirect_uri` は **Strongly recommended** パラメータ
- 省略した場合、GitHub OAuth App 設定の callback URL にリダイレクトされる
- 指定した場合の検証ルール:
  - ホスト（サブドメインを除く）とポートが callback URL と**完全一致**する必要がある
  - パスは callback URL の**サブディレクトリ**である必要がある
  
```
CALLBACK: http://example.com/path
GOOD: http://example.com/path
GOOD: http://example.com/path/subdir/other
GOOD: http://oauth.example.com/path       ← サブドメイン違いはOK
BAD:  http://example.com:8080/path         ← ポート違いはNG
BAD:  http://example.com/bar               ← パスが違うとNG
```

- ループバック URL (`127.0.0.1`) の場合はポート違いも許可される

**BoardFlow への適用**:
- GitHub OAuth App の callback URL を `http://localhost:3000/api/v1/auth/callback` に設定
- API が `redirect_uri=http://localhost:3000/api/v1/auth/callback` を authorize URL に付与
- GitHub はフロントエンドドメインにリダイレクトする

### 3. ベストプラクティス

| 方式 | メリット | デメリット | 適用性 |
|---|---|---|---|
| **Cookie + redirect_uri でフロントエンド経由** | シンプル、既存アーキテクチャに合致 | redirect_uri の検証ルールを理解する必要あり | ✅ 推奨 |
| DB ベース state 管理 | ドメイン制約なし | state テーブル追加、TTL 管理が必要 | △ オーバーエンジニアリング |
| SameSite=None; Secure | 最小変更 | HTTPS 必須、CSRF 保護が弱まる | ✗ セキュリティ低下 |
| Cookie Domain 属性で共通ドメイン指定 | 同一親ドメインなら可能 | localhost で使えない、ドメイン構成に依存 | ✗ 開発環境で使えない |

IETF OAuth BCP (draft-ietf-oauth-browser-based-apps) では、BFF (Backend-for-Frontend) パターンとして、フロントエンドドメインをすべての OAuth フローの窓口とすることが推奨されている。

## BoardFlow への示唆

### 推奨アプローチ: `redirect_uri` によるフロントエンドドメイン経由

提案されたソリューションは**正しく、推奨される**。完全なフロー:

```
[Browser] → GET localhost:3000/api/v1/auth/login
  ↓ (Next.js rewrite)
[API localhost:8080] → 302 GitHub + Set-Cookie: boardflow_oauth_state=XXX
  ↓ (Next.js passthrough)
[Browser] ← 302 + Set-Cookie (Cookie は localhost:3000 に保存)
  ↓
[Browser] → GET github.com/login/oauth/authorize?...&redirect_uri=http://localhost:3000/api/v1/auth/callback
  ↓ (ユーザーが認可)
[GitHub] → 302 http://localhost:3000/api/v1/auth/callback?code=...&state=XXX
  ↓
[Browser] → GET localhost:3000/api/v1/auth/callback?code=...&state=XXX
              Cookie: boardflow_oauth_state=XXX  ← Cookie 送信される！
  ↓ (Next.js rewrite)
[API localhost:8080] ← Cookie state == query state → 認証成功
```

### 必要な変更

1. **`routes/auth.rs` の `login` ハンドラ**:
   - `redirect_uri` を GitHub authorize URL に追加
   - `BOARDFLOW_APP_DOMAIN` (`AppDomain`) を注入して `{app_domain}/api/v1/auth/callback` を構築
2. **GitHub OAuth App 設定**:
   - callback URL をフロントエンドドメインに変更（例: `http://localhost:3000/api/v1/auth/callback`）
3. **本番環境**: `BOARDFLOW_APP_DOMAIN=https://app.boardflow.example.com` を設定

## 採用/不採用判断

**採用**: `redirect_uri` パラメータによるフロントエンドドメイン経由方式

理由:
- 既存の Next.js rewrite プロキシアーキテクチャに自然に統合される
- Cookie ベースの CSRF 防御を維持できる
- DB スキーマ変更不要
- `BOARDFLOW_APP_DOMAIN` 環境変数が既に存在する

## 制約と pitfall

1. **GitHub OAuth App の callback URL 設定**: フロントエンドドメインに変更する必要がある。開発環境と本番環境で OAuth App が異なる場合は両方変更
2. **ポート一致の必要性**: `redirect_uri` のポートは callback URL と一致する必要がある（ループバック除く）
3. **HTTPS**: 本番環境では `Secure` フラグを Cookie に追加すべき。現在は未設定
4. **`redirect_uri` の URL エンコード**: authorize URL に含める際に適切にエンコードする必要がある
5. **Next.js rewrite の redirect passthrough**: Next.js は 302 レスポンスをフォローせずブラウザに返すため、callback 後の API の 302 リダイレクト（`/` や `redirect_to` パスへ）もフロントエンドドメインの相対パスとして正しく動作する
6. **token exchange の `redirect_uri`**: GitHub の access_token エンドポイントにも `redirect_uri` を送ることが Strongly recommended。code 発行時の URI と照合される

## 未解決の疑問

1. 現在の GitHub OAuth App の callback URL 設定値は何か？（API ドメインかフロントエンドドメインか）
2. 開発環境と本番環境で別の OAuth App を使っているか？
3. `BOARDFLOW_APP_DOMAIN` に末尾スラッシュが含まれる可能性は？（URL 結合時のバグ防止）

## 参照URL

- https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps
- https://github.com/serverless-nextjs/serverless-next.js/issues/929
- https://github.com/vercel/next.js/discussions/17325
- https://github.com/vercel/next.js/issues/90202
- https://datatracker.ietf.org/doc/html/draft-ietf-oauth-browser-based-apps
- https://nextjs.org/docs/pages/api-reference/config/next-config-js/rewrites
