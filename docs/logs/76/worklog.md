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

## レビュー結果

### 2026-05-04: Review phase

#### 総評

- 実装方針そのものは妥当。`state` cookie と query `state` の照合は維持されており、`redirect_uri` を authorize と token exchange の双方に渡している点も外部調査と整合する。
- 一方で、この修正は GitHub OAuth App 側の callback URL をフロントエンドドメインへ変更して初めて成立するが、その運用手順が README / 恒久ドキュメントへ反映されていない。
- また、追加テストは `login` ハンドラに偏っており、`callback` / `logout` 側の今回変更した経路を自動検証できていない。

#### 判定

- `pr_ready: false`

#### 指摘事項

1. **major**: 運用上必須の GitHub OAuth App callback URL 変更がドキュメント化されていない。調査と計画では `redirect_uri` 方式の成立条件として GitHub OAuth App の callback URL をフロントエンドドメインへ変更する必要があると明記されているが、README 等の利用者向けドキュメントには反映がない。この状態だとコードをデプロイしても OAuth App 設定が旧 API ドメインのまま残り、Issue の不具合が再現し続ける可能性が高い。
2. **minor**: `BOARDFLOW_APP_DOMAIN` の空文字・不正値を受け入れてしまう。`optional_env_or()` は未設定時のみデフォルトへフォールバックし、空文字はそのまま通すため、`auth.rs` 側では `redirect_uri=/api/v1/auth/callback` のような無効値を組み立てうる。Issue のレビュー観点に含まれている空ドメインのエッジケースが未解消。
3. **minor**: テスト追加は `login` の Location / cookie に限定され、今回変更した `callback` の token exchange `redirect_uri`、callback 成功時 cookie の `Secure` 条件、`logout` の `Secure` 条件が未検証。受け入れ条件では callback 側の `redirect_uri` 付与も明示されているため、回帰検知として不足がある。

#### 必須修正

1. GitHub OAuth App の callback URL をフロントエンドドメインへ変更する必要があることを README か運用ドキュメントへ追記する。少なくとも開発環境・本番環境の設定例と、`BOARDFLOW_APP_DOMAIN` と一致させる必要がある旨を明記する。
2. `BOARDFLOW_APP_DOMAIN` が空文字または絶対 URL でない場合に起動時またはルータ構築時に失敗させる。最低限、空文字をデフォルト扱いにするか、明示的な設定エラーにする。
3. `callback` / `logout` の今回変更箇所を対象にしたテストを追加する。少なくとも token exchange に `redirect_uri` が渡ること、HTTPS 時の session/clear cookie に `Secure` が付くこと、logout cookie でも同条件が適用されることを確認する。

#### 任意改善

1. `app_domain.trim_end_matches('/')` 済みの値から `Secure` 判定も行い、URL 正規化ロジックを一箇所へ寄せる。
2. `BOARDFLOW_APP_DOMAIN` を URL 型で保持する helper を追加し、scheme / host の妥当性を config crate 側で共通化する。

#### テスト不足

1. GitHub token exchange リクエストに `redirect_uri` が含まれることの確認
2. callback 成功時の `boardflow_session` / `boardflow_oauth_state` / `boardflow_redirect_to` cookie の `Secure` 条件確認
3. `logout` の cookie clear に対する HTTPS / HTTP 分岐確認
4. `BOARDFLOW_APP_DOMAIN` 空文字・不正 URL の異常系確認

#### ドキュメント確認

- research メモと worklog では callback URL 手動変更の必要性が明記されている
- README / 利用者向け恒久ドキュメントにはその運用手順が見当たらない

#### PR/完了結果

- レビュー時点では PR 作成非推奨 (`pr_ready: false`)

#### 残リスク

- GitHub OAuth App 設定が環境ごとにズレると、本修正コードが入っていても state mismatch ではなく callback mismatch として障害化する
- 現状テストだけでは callback / logout 側の将来回帰を捕捉しにくい

---

## レビュー指摘対応

### 2026-05-04: Review feedback fix

#### 対応内容

| 指摘 | 対応 |
|------|------|
| major: README に OAuth App callback URL 設定ドキュメントがない | README.md に「GitHub OAuth App 設定」セクションを追加。開発/本番の callback URL 例、`BOARDFLOW_APP_DOMAIN` との一致要件、Next.js rewrite 前提を明記 |
| minor: 空文字 `BOARDFLOW_APP_DOMAIN` バリデーション | `crates/api/src/lib.rs` で `domain_str` が空文字の場合に `"http://localhost:3000"` へフォールバック |
| minor: callback/logout テスト追加 | `cookie_secure_flag()` ヘルパー関数を `auth.rs` に切り出し、ユニットテスト4件を追加。`auth_test.rs` にも統合テスト4件（secure_flag helper 3件 + 空ドメインfallback 1件）を追加 |

#### 変更ファイル

| ファイル | 変更内容 |
|----------|----------|
| `README.md` | GitHub OAuth App 設定セクション追加 |
| `crates/api/src/lib.rs` | 空文字フォールバック追加 |
| `crates/api/src/routes/auth.rs` | `cookie_secure_flag()` pub 関数を追加、login/callback/logout のインライン判定を関数呼び出しに置換、ユニットテスト4件追加 |
| `crates/api/tests/auth_test.rs` | `cookie_secure_flag` import追加、統合テスト4件追加 |

#### テスト結果

```
auth_test.rs: 21 passed; 0 failed
routes::auth::tests: 14 passed; 0 failed
```

#### 残リスク

- callback/logout のエンドツーエンドテスト（DB + GitHub API mock）は環境未整備のため未追加。`cookie_secure_flag` ヘルパーのユニットテストでロジックカバレッジは確保
- GitHub OAuth App の実際の callback URL 変更は手動運用作業として残る

---

## レビュー結果

### 2026-05-04: Review phase

#### Issueまでの経緯

- 対象は Issue #76 のみ。OAuth state mismatch を API / frontend のクロスドメイン構成で解消する変更をレビュー
- 前回レビューの指摘は README の OAuth 設定手順、空文字 `BOARDFLOW_APP_DOMAIN` フォールバック、callback/logout の回帰検知不足

#### ユーザー要望

- 前回指摘が解消されているか確認
- 新たなセキュリティ問題がないか確認
- コード品質と PR 作成可否を判定

#### 調査結果

- 実装は `login` / `callback` で `redirect_uri={BOARDFLOW_APP_DOMAIN}/api/v1/auth/callback` を一貫して使用し、`SameSite=Lax` を維持したまま `Secure` フラグだけを scheme 条件で付与している
- 外部調査および `docs/external/oauth-state-cross-domain.md` の方針は妥当。`SameSite=None` に逃げず、frontend ドメイン経由に寄せる設計は妥当
- `cargo test -p boardflow-api --test auth_test` はレビュー中に再実行し、21 件すべて成功を確認
- 一方で README の OAuth callback 設定例とローカル開発手順のポートが矛盾している。OAuth 設定では開発 callback を `http://localhost:3000/api/v1/auth/callback` と案内しているが、同じ README のローカル開発手順は frontend を `http://localhost:3001` で起動する前提になっている
- README の `BOARDFLOW_APP_DOMAIN` デフォルト説明も実装と不一致。README は `https://boardflow.example.com` と記載しているが、実装は `http://localhost:3000` をデフォルトとしている

#### 計画との差分

- 実装計画どおり `redirect_uri` 追加、Secure 条件付与、空文字フォールバック、README 追記、テスト追加は行われている
- ただし README 更新は「手順を追加した」点では達成している一方、内容の整合性までは満たしていない

#### 実装内容レビュー

- `crates/api/src/routes/auth.rs` の変更は妥当。state cookie を frontend ドメインに残す設計に対して `redirect_uri` の注入位置も正しい
- `crates/api/src/lib.rs` の空文字フォールバックは前回指摘の最小修正として有効
- `cookie_secure_flag()` の抽出で login/callback/logout の cookie 属性判定が一箇所に寄り、保守性は改善している

#### テスト結果

- `cargo test -p boardflow-api --test auth_test`: 21 passed, 0 failed
- 申告どおり auth 系の回帰テストは通過

#### ドキュメント確認

- `README.md` に OAuth 設定セクション自体は追加済み
- ただし callback URL 例、`BOARDFLOW_APP_DOMAIN` の説明、ローカル開発手順の 3 点が相互に整合していない

#### レビュー結果

- `pr_ready: false`

##### 指摘事項

1. **major**: `README.md` の OAuth callback URL 例が、同じ README のローカル開発手順と矛盾している。OAuth 設定セクションは開発環境 callback を `http://localhost:3000/api/v1/auth/callback` と案内している一方、ローカル開発手順は frontend を `http://localhost:3001` で起動する前提になっているため、README どおり設定すると Issue #76 の再現条件をそのまま踏む。対象箇所: `README.md` の `BOARDFLOW_APP_DOMAIN` 説明・OAuth 設定例・Frontend ローカル開発手順。

##### 必須修正

1. `README.md` の OAuth callback URL 例と `BOARDFLOW_APP_DOMAIN` の説明を、実際のローカル開発ポート構成に合わせて統一する。frontend を `3001` で起動する前提を維持するなら callback 例も `http://localhost:3001/api/v1/auth/callback` に修正し、`BOARDFLOW_APP_DOMAIN` もその値を設定すべきことを明記する。逆に `3000` を正とするなら、ローカル開発手順側を修正する。
2. `README.md` の `BOARDFLOW_APP_DOMAIN` デフォルト値説明を、実装 (`crates/api/src/lib.rs`) と一致させる。

##### 任意改善

1. `BOARDFLOW_APP_DOMAIN` を URL 型として起動時に妥当性検証する helper を別 Issue で検討するとよい。今回は空文字フォールバックまでで十分だが、不正 URL 文字列は依然としてそのまま通る。

##### テスト不足

1. `callback` / `logout` の cookie `Secure` 付与を HTTP レスポンスとして確認する統合テストは未追加。今回の変更は helper 抽出でロジック共有されているためリスクは低いが、将来 handler ごとに cookie 属性を崩した場合の検知は弱い。
2. GitHub token exchange に `redirect_uri` が常に送られることを確認するテストは未整備。

##### plan / research / docs との不整合

1. research と実装は整合しているが、README の開発向け callback URL 例が現在のローカル開発手順と不整合。
2. README の `BOARDFLOW_APP_DOMAIN` デフォルト値記載が `crates/api/src/lib.rs` の実装と不整合。

#### PR/完了結果

- PR 作成は現時点では非推奨。README の整合を修正してからであればレビュー観点 1 の未解消点を解消できる

#### 残リスク

- README を見て設定した開発者が誤った callback URL を GitHub OAuth App に登録すると、Issue #76 と同種の state mismatch を再度踏む
- callback/logout の E2E 検証は引き続き未整備

### 2026-05-04: Re-review phase after follow-up fixes

#### Issueまでの経緯

- 対象は Issue #76 のみ。前回レビューで指摘した README と `.env.example` のポート不整合、`BOARDFLOW_APP_DOMAIN` 説明差分の修正後状態を再確認
- 今回の確認対象は README の OAuth 設定手順、前回指摘の解消状況、PR 作成可否

#### ユーザー要望

- README の OAuth 設定手順がローカル開発構成 API:3000 / Frontend:3001 と整合しているか確認
- 前回指摘がすべて解消されたか確認
- PR 作成可能か判定

#### 調査結果

- `README.md` は `BOARDFLOW_APP_DOMAIN` の default を `http://localhost:3000` と記載しており、`crates/api/src/lib.rs` の実装と一致している
- `README.md` の OAuth 設定表は開発環境 callback を `http://localhost:3001/api/v1/auth/callback`、`BOARDFLOW_APP_DOMAIN` を `http://localhost:3001` と案内しており、同ファイル内の Frontend ローカル開発手順と整合している
- `.env.example` も `BOARDFLOW_APP_DOMAIN=http://localhost:3001` に更新されており、ローカル開発時の設定例として一貫している
- `crates/api/src/routes/auth.rs` は `redirect_uri={BOARDFLOW_APP_DOMAIN}/api/v1/auth/callback` を login / callback の双方で使い、Cookie の `Secure` 付与条件も維持している
- ブランチ `fix/76-oauth-state-mismatch` 上で `mise exec -- cargo test -p boardflow-api --test auth_test` を再実行し、21 passed を確認
- 同じく `mise exec -- cargo check -p boardflow-api` は成功を確認

#### 計画との差分

- 前回レビューで必須修正として挙げた README callback URL、`BOARDFLOW_APP_DOMAIN` 説明、`.env.example` の 3 点は解消済み
- 実装計画で想定した callback/login の `redirect_uri` 一貫性、Secure フラグ条件、テスト追加の方向性とも矛盾はない

#### 実装内容レビュー

- 実装の根幹であるクロスドメイン OAuth state mismatch 対策は妥当
- README と環境変数例が現行のローカル開発構成に追従したため、前回のドキュメント起因の再発リスクは大きく下がった
- 前回指摘した内容に関して、新たなブロッカーは見当たらない

#### テスト結果

- `mise exec -- cargo test -p boardflow-api --test auth_test`: 21 passed, 0 failed
- `mise exec -- cargo check -p boardflow-api`: success

#### ドキュメント確認

- `README.md` の利用者向け手順は Issue #76 の目的に対して十分に整合している
- 一方で research 成果物 `docs/external/oauth-state-cross-domain.md` には旧ローカル例 (`localhost:3000`) が残っている。ただし research メモであり、今回の PR 可否を左右するものではない

#### レビュー結果

- `pr_ready: true`

##### 指摘事項

1. blocking な指摘はなし

##### 必須修正

1. なし

##### 任意改善

1. `docs/external/oauth-state-cross-domain.md` のローカル callback 例を現行の開発構成に合わせて `3001` ベースへ更新するか、当時の検証条件である旨を注記すると research / README の読み替えコストを下げられる

##### テスト不足

1. callback/logout の E2E 挙動は依然として統合テストでは直接確認していないが、今回の PR 可否を下げるほどの不足ではない

##### plan / research / docs との不整合

1. 利用者向け docs と実装の不整合は解消済み
2. research メモのみ旧ポート例が残存している

#### PR/完了結果

- 前回レビュー指摘は解消済みであり、Issue #76 は PR 作成可能と判断

#### 残リスク

- GitHub OAuth App の callback URL を実運用環境で `BOARDFLOW_APP_DOMAIN` と一致させない場合、実装が正しくても OAuth は失敗する
- research 成果物の旧ポート例をそのまま参照すると、現行 README と読み比べが必要になる

---

## ドキュメント確認

### 2026-05-04: Docs review phase

#### Issueまでの経緯

- 対象は Issue #76 のみ。OAuth state mismatch 修正について、実装概要・research 成果物・README 更新・既存 docs の整合性を確認
- 今回の確認対象は `docs/external/oauth-state-cross-domain.md`、`README.md`、`.env.example`、`docs/backend/api.md`、`docs/spec.md`、`docs/technology.md`

#### ユーザー要望

- ドキュメントの正確性と整合性を確認
- README の OAuth 設定手順が実装と一致しているか確認
- 既存ドキュメントとの矛盾と更新漏れを洗い出す
- PR 作成可否を `docs_ready` で判定

#### 調査結果

- 実装 (`crates/api/src/routes/auth.rs`) は `login` / `callback` の双方で `redirect_uri={BOARDFLOW_APP_DOMAIN}/api/v1/auth/callback` を利用し、cookie の `Secure` は app domain が `https://` のときだけ付与する
- `README.md` は開発時の frontend `3001` / API `3000` 構成、GitHub OAuth callback URL 例、`.env` の `BOARDFLOW_APP_DOMAIN=http://localhost:3001` 案内が一致しており、実装とも整合している
- `.env.example` も `BOARDFLOW_APP_DOMAIN=http://localhost:3001` に更新されており、README のローカル開発手順と整合している
- 一方で `docs/external/oauth-state-cross-domain.md` には `http://localhost:3000/api/v1/auth/callback` を前提にしたローカル例が残っており、README / `.env.example` / 現行ローカル開発構成と不整合
- `docs/backend/api.md` の Auth API 節は login/callback の概要はあるが、今回の修正で利用者・開発者にとって重要になった `redirect_uri` による frontend ドメイン経由、token exchange でも同じ `redirect_uri` を送ること、cookie の `Secure` が `BOARDFLOW_APP_DOMAIN` の scheme に依存することが反映されていない
- `docs/spec.md` と `docs/technology.md` には今回の変更で直接矛盾する記述は見当たらない

#### 計画との整合

- 計画にあった README 追記は実施済みで、内容も現行実装と整合している
- research 成果物は方針自体は実装と整合しているが、ローカル例のポートだけが README と食い違う
- 既存 docs 更新の観点では `docs/backend/api.md` の追随が不足している

#### 実装内容との照合

- `README.md` の「GitHub OAuth App 設定」は、`redirect_uri` を frontend ドメインに寄せる現実装と一致している
- `README.md` の `BOARDFLOW_APP_DOMAIN` 説明は `crates/api/src/lib.rs` の default と一致している
- `docs/backend/api.md` は Auth API の canonical に近い位置付けだが、今回の動作変更が反映されておらず、実装との差分が残っている

#### テスト結果

- ドキュメントレビューのため追加テスト実行はなし
- 参照した既存 worklog 上では `auth_test` と `cargo check` の成功が記録済み

#### レビュー結果

- `docs_ready: false`

##### 必須修正

1. `docs/external/oauth-state-cross-domain.md` のローカル callback URL / `redirect_uri` / フロー図の例を、現行の開発前提 (`BOARDFLOW_APP_DOMAIN=http://localhost:3001`) に合わせて更新するか、当時の検証条件である旨を明記する。Issue 76 の research 成果物として残す以上、README と逆のポート例を未注記で残すのは不整合。
2. `docs/backend/api.md` の Auth API 節に、今回の仕様変更を反映する。少なくとも以下は明記が必要。
   - login が GitHub authorize URL に `redirect_uri={BOARDFLOW_APP_DOMAIN}/api/v1/auth/callback` を含めること
   - callback の token exchange にも同じ `redirect_uri` を送ること
   - `boardflow_oauth_state` / `boardflow_redirect_to` / `boardflow_session` cookie の `Secure` は `BOARDFLOW_APP_DOMAIN` が `https://` のときだけ付与されること
   - OAuth callback は frontend ドメイン経由を前提とすること

##### 任意改善

1. `docs/backend/api.md` の auth 節に `BOARDFLOW_APP_DOMAIN` を OAuth callback / cookie 属性の決定要因として短く追記すると、README と API 仕様書の役割分担が明確になる
2. research 成果物に「README が運用手順の正、research は検証メモ」という注記があると、今後の読み替えコストを下げられる

##### 不整合のあるドキュメント

1. `docs/external/oauth-state-cross-domain.md`: ローカル例が `localhost:3000` ベースのままで、README / `.env.example` と不整合
2. `docs/backend/api.md`: Auth API の動作説明が Issue #76 実装に追随していない

##### 不足しているドキュメント

1. Auth API 仕様書上での `redirect_uri` と条件付き `Secure` cookie の説明

##### 外部調査メモに関する指摘

1. `docs/external/oauth-state-cross-domain.md` の根拠 URL 自体は十分だが、BoardFlow への適用例が現行のローカル開発構成に追随していない
2. research の結論は妥当で、採用判断と実装方針も一致している。問題は結論ではなく、例示値のメンテナンス不足

#### PR/完了結果

- ドキュメント観点では現時点で PR 作成は非推奨。README 単体では整っているが、Issue に紐づく research と既存 API 仕様書を含めると docs セット全体の整合が未完了

#### 残リスク

- 開発者が `docs/external/oauth-state-cross-domain.md` を先に参照すると、README と異なる callback URL 例を採用する可能性がある
- `docs/backend/api.md` を参照して Auth API を理解する開発者には、frontend ドメイン経由の callback と条件付き `Secure` cookie の仕様変更が伝わらない

---

## ドキュメント再確認

### 2026-05-04: Docs re-review after follow-up fixes

#### Issueまでの経緯

- 対象は Issue #76 のみ
- 今回の再確認対象は、前回 docs 指摘だった `docs/external/oauth-state-cross-domain.md` と `docs/backend/api.md`
- ユーザー申告では external メモのローカル例を `Frontend:3001 / API:3000` に統一し、backend API 仕様に `redirect_uri` と条件付き `Secure` cookie の説明を追記済み

#### ユーザー要望

- 前回指摘が解消されているかを確認
- `docs_ready` を再判定

#### 調査結果

- `docs/backend/api.md` の Auth API 節には、login での `redirect_uri={BOARDFLOW_APP_DOMAIN}/api/v1/auth/callback`、callback の token exchange に同じ `redirect_uri` を送ること、cookie の `Secure` が `BOARDFLOW_APP_DOMAIN` の scheme 条件で付与されることが追記されており、前回指摘は解消されている
- `README.md` と `.env.example` は現行のローカル開発構成である frontend `localhost:3001` / API `localhost:3000` と整合している
- 一方で `docs/external/oauth-state-cross-domain.md` には、フロー図や冒頭のローカル挙動説明は `3001/3000` に更新されているものの、`BoardFlow への適用` 節に以下の旧例が残っている
   - GitHub OAuth App の callback URL を `http://localhost:3000/api/v1/auth/callback` に設定
   - API が `redirect_uri=http://localhost:3000/api/v1/auth/callback` を authorize URL に付与
- この 2 行は README の運用手順および同ファイル内の他の説明と矛盾しており、「すべてのフロー図と例を現行開発構成に更新」という条件は未達

#### ドキュメント確認

- `docs/backend/api.md`: 修正済み。前回指摘は解消
- `docs/external/oauth-state-cross-domain.md`: 一部未修正。旧 callback URL 例が残存

#### レビュー結果

- `docs_ready: false`

##### 必須修正

1. `docs/external/oauth-state-cross-domain.md` の `BoardFlow への適用` 節に残っている callback URL / `redirect_uri` の旧例を、現行構成である `http://localhost:3001/api/v1/auth/callback` に修正する

##### 任意改善

1. external メモ内に `BOARDFLOW_APP_DOMAIN=http://localhost:3001` を明示すると、README と読み比べなくても文脈が閉じる

##### 不整合のあるドキュメント

1. `docs/external/oauth-state-cross-domain.md`

##### 不足しているドキュメント

1. なし

##### 外部調査メモに関する指摘

1. 根拠 URL と採用判断自体は妥当だが、BoardFlow への適用例だけが現行の開発構成に追随していない

#### PR/完了結果

- ドキュメント観点では、前回指摘は一部のみ解消
- `docs/backend/api.md` の修正は確認できたが、external メモの旧例が残っているため PR 作成可とは判定しない

#### 残リスク

- external メモを参照した開発者が `localhost:3000` を callback URL に設定すると、Issue #76 の再発条件をそのまま踏む

---

## ドキュメント再々確認

### 2026-05-04: Docs re-review after external memo correction

#### Issueまでの経緯

- 対象は Issue #76 のみ
- 今回の確認対象は、前回 docs blocking 指摘だった `docs/external/oauth-state-cross-domain.md` の `BoardFlow への適用` 節と、整合先となる `README.md`、`.env.example`、`docs/backend/api.md`
- ユーザー申告では external メモ 48-49 行の callback URL / `redirect_uri` 例を `localhost:3001` ベースへ修正済み

#### ユーザー要望

- 前回指摘の external メモ旧ポート例が解消されているか確認
- ドキュメント全体の整合性を再確認
- `docs_ready` を再判定

#### 調査結果

- `docs/external/oauth-state-cross-domain.md` の `BoardFlow への適用` 節は、GitHub OAuth App の callback URL と authorize URL の `redirect_uri` 例がともに `http://localhost:3001/api/v1/auth/callback` に更新されている
- 同ファイルの冒頭フロー説明、フロー図、後続の適用例も `Frontend: localhost:3001 / API: localhost:3000` に統一されており、同一ファイル内の不整合は解消されている
- `README.md` は開発環境の callback URL を `http://localhost:3001/api/v1/auth/callback`、`BOARDFLOW_APP_DOMAIN` を `http://localhost:3001` と案内しており、external メモと整合している
- `.env.example` も `BOARDFLOW_APP_DOMAIN=http://localhost:3001` となっており、README と整合している
- `docs/backend/api.md` には login/callback での `redirect_uri={BOARDFLOW_APP_DOMAIN}/api/v1/auth/callback` と条件付き `Secure` cookie の説明があり、Issue #76 の実装方針と一致している
- 現行 docs 上で `http://localhost:3000/api/v1/auth/callback` を案内している箇所は、今回の確認対象範囲では見当たらない

#### ドキュメント確認

- `docs/external/oauth-state-cross-domain.md`: 修正済み。前回指摘は解消
- `README.md`: 修正済み。ローカル開発手順と OAuth 設定例が整合
- `.env.example`: 修正済み。README と整合
- `docs/backend/api.md`: 修正済み。Auth API 仕様は現実装と整合

#### レビュー結果

- `docs_ready: true`

##### 必須修正

1. なし

##### 任意改善

1. なし

##### 不整合のあるドキュメント

1. なし

##### 不足しているドキュメント

1. なし

##### 外部調査メモに関する指摘

1. 根拠 URL と採用判断、BoardFlow への適用例のいずれも現行の開発構成と整合している

#### PR/完了結果

- ドキュメント観点での blocking 指摘は解消済み
- Issue #76 は docs 観点で PR 作成可能と判断

#### 残リスク

- GitHub OAuth App の実運用設定が `BOARDFLOW_APP_DOMAIN` と一致しない場合、実装と docs が正しくても OAuth は失敗する
