# Axum 0.8 での複数 extractor の使い方

## 要約

Axum 0.8 で `AuthenticatedToken`（カスタム `FromRequestParts` extractor）と `Json<T>`（`FromRequest` extractor）を同じ handler で組み合わせる際のパターン、引数順序の制約、エラーハンドリングの方針をまとめる。

## 確認した情報

### Axum の extractor 分類

Axum 0.8 では extractor は2つのトレイトに分類される:

1. **`FromRequestParts`**: リクエストの metadata（ヘッダー、URI、extensions 等）から抽出。body を消費しない。複数使用可能。
2. **`FromRequest`**: リクエスト全体（body 含む）から抽出。body を消費するため、handler 引数の**最後に1つだけ**配置可能。

### 引数順序の制約

Axum は handler 引数を**左から右の順**で処理する。body を消費する extractor（`FromRequest` 実装）は**最後の引数**でなければならない。`FromRequestParts` を実装した extractor は任意の順序で複数配置できるが、body extractor より前に置く必要がある。

```rust
// ✅ 正しい: FromRequestParts が先、FromRequest (Json) が最後
async fn handler(
    auth: AuthenticatedToken,       // FromRequestParts
    State(pool): State<PgPool>,     // FromRequestParts
    Json(body): Json<PlanRequest>,  // FromRequest — 必ず最後
) -> Result<Json<PlanResponse>, AppError> {
    // ...
}

// ❌ コンパイルエラー: Json が最後でない
async fn bad_handler(
    Json(body): Json<PlanRequest>,  // FromRequest
    auth: AuthenticatedToken,       // FromRequestParts — Json の後に来れない
) -> ... {
    // ...
}
```

### BoardFlow での具体的パターン

`AuthenticatedToken` は既に `FromRequestParts` を実装している（`crates/api/src/extractors/auth.rs`）。`PgPool` は `State` extractor 経由で取得。`Json<T>` は `FromRequest` を実装。

```rust
#[utoipa::path(
    post,
    path = "/api/v1/runs/plan",
    request_body = PlanRequest,
    responses(
        (status = 200, description = "Plan response", body = PlanResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 400, description = "Validation failed", body = ErrorResponse),
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn plan(
    auth: AuthenticatedToken,          // 1. カスタム extractor (FromRequestParts)
    State(pool): State<PgPool>,        // 2. State extractor (FromRequestParts)
    Json(req): Json<PlanRequest>,      // 3. Body extractor (FromRequest) — 最後
) -> Result<Json<PlanResponse>, AppError> {
    let token = auth.0;
    // token.repository_id でアクセス制御
    // req でリクエストボディを使用
    // pool でDB操作
    todo!()
}
```

### State extractor の省略パターン

`PgPool` を直接 handler 引数で受ける代わりに、`AuthenticatedToken` 内部で既に pool を使っている。handler 側でも pool が必要な場合は `State(pool): State<PgPool>` を追加する。

### エラーハンドリング

各 extractor の Rejection 型:
- `AuthenticatedToken` → `AppError`（`crates/api/src/error.rs` で定義済み）
- `State<PgPool>` → Infallible（State は app 構築時に必ず設定されるため）
- `Json<T>` → `JsonRejection`

handler の戻り値が `Result<T, AppError>` の場合、`AuthenticatedToken` のエラーは自動的に `AppError` として返る。`Json` の `JsonRejection` は `AppError` への変換（`From<JsonRejection> for AppError`）を実装する必要がある。

```rust
impl From<axum::extract::rejection::JsonRejection> for AppError {
    fn from(rejection: axum::extract::rejection::JsonRejection) -> Self {
        AppError {
            code: ErrorCode::ValidationFailed,
            message: rejection.body_text(),
            details: None,
            request_id: String::new(), // request_id は middleware で設定済み
        }
    }
}
```

### 3つ以上の FromRequestParts extractor の組み合わせ

Axum 0.8 では `FromRequestParts` を実装した extractor を理論上16個まで handler 引数として使える（タプルの impl が T1〜T16 まで）。実用上は問題にならない。

```rust
async fn complex_handler(
    method: Method,                    // FromRequestParts
    headers: HeaderMap,                // FromRequestParts
    auth: AuthenticatedToken,          // FromRequestParts (カスタム)
    State(pool): State<PgPool>,        // FromRequestParts
    Json(body): Json<SomeRequest>,     // FromRequest — 最後
) -> impl IntoResponse {
    // ...
}
```

## BoardFlow への示唆

Plan API handler の引数構成:

```rust
async fn plan(
    auth: AuthenticatedToken,
    State(pool): State<PgPool>,
    Json(req): Json<PlanRequest>,
) -> Result<Json<PlanResponse>, AppError>
```

この順序は Axum の制約を満たし、BoardFlow の既存パターン（`AuthenticatedToken` + `State<PgPool>`）と一致する。`JsonRejection` → `AppError` の変換を追加することで、不正な JSON body に対しても統一的なエラーレスポンスが返る。

## 採用/不採用判断

**採用**: `AuthenticatedToken` + `State<PgPool>` + `Json<PlanRequest>` の3引数パターンで実装する。Axum 公式ドキュメントの推奨パターンに準拠。

## 制約と pitfall

- `Json<T>` は必ず handler 引数の**最後**に配置する（コンパイルエラーになるので発見は容易）
- `FromRequestParts` を実装したカスタム extractor のエラー型は handler の戻り値のエラー型と互換性が必要
- `AuthenticatedToken` の Rejection が `AppError` であるため、handler の戻り値が `Result<_, AppError>` なら変換不要
- `JsonRejection` は `AppError` への `From` 実装が別途必要（現時点では未実装）
- Extractor は左から右に順番に実行されるため、認証（`AuthenticatedToken`）が body parse（`Json`）より先に実行される — これはセキュリティ上望ましい（認証失敗時に body を parse しない）
- `State` extractor は `PgPool: FromRef<S>` の bound が必要だが、現在 `create_app` で `with_state(pool)` を使用しているため問題ない

## 未解決の疑問

- `JsonRejection` → `AppError` 変換で `request_id` をどう取得するか。現在の `AuthenticatedToken` は `parts.extensions.get::<RequestId>()` で取得しているが、`Json` の rejection 発生時に同じ機構が使えるか要確認（middleware の実行順序次第）。ただしこれは実装段階で解決可能な問題。

## 参照URL

- https://docs.rs/axum/0.8/axum/extract/index.html （Axum extractor 公式ドキュメント）
- https://docs.rs/axum/0.8/axum/extract/trait.FromRequestParts.html （FromRequestParts トレイト）
- https://docs.rs/axum/0.8/axum/extract/trait.FromRequest.html （FromRequest トレイト）
