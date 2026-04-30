# utoipa-axum での POST リクエスト body 定義パターン

## 要約

utoipa 5.x + utoipa-axum 0.2 で `#[utoipa::path]` マクロを使ってPOSTエンドポイントを定義する際のパターンをまとめる。`request_body` 属性と Axum の `Json<T>` extractor の組み合わせ、`security` 属性による Bearer 認証の指定方法を含む。

## 確認した情報

### 基本パターン: `request_body = Type`

最もシンプルなPOST定義。`request_body` には `ToSchema` を derive した型を指定する。handler 関数側では `Json<T>` extractor を使う。utoipa はマクロ上の `request_body` 型と handler の引数型を独立に扱うため、両者が一致していなくてもコンパイルは通るが、OpenAPI spec と実装の一致はユーザー責任である。

```rust
#[derive(Deserialize, Serialize, ToSchema)]
struct OrderRequest {
    name: String,
}

#[derive(Serialize, ToSchema)]
struct Order {
    id: i32,
    name: String,
}

#[utoipa::path(
    post,
    path = "/orders",
    request_body = OrderRequest,
    responses(
        (status = 200, description = "Order created", body = Order),
    )
)]
async fn create_order(Json(req): Json<OrderRequest>) -> Json<Order> {
    Json(Order { id: 1, name: req.name })
}
```

### 高度なパターン: `request_body(content = Type, ...)`

description、content_type、example を追加する場合は括弧形式を使う。

```rust
#[utoipa::path(
    post,
    path = "/runs/plan",
    request_body(
        content = PlanRequest,
        description = "Plan request with project candidates and tree hashes",
        content_type = "application/json"
    ),
    responses(
        (status = 200, description = "Plan response", body = PlanResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn plan(
    auth: AuthenticatedToken,
    Json(req): Json<PlanRequest>,
) -> Result<Json<PlanResponse>, AppError> {
    // ...
}
```

### security 属性

`security(("bearer_auth" = []))` でエンドポイント単位のセキュリティ要件を指定する。`bearer_auth` は `OpenApi` derive の `SecurityAddon` modifier で登録した SecurityScheme の名前と一致させる必要がある。現在の BoardFlow コードでは既に `SecurityAddon` で `bearer_auth` が登録済み。

### utoipa-axum でのルーティング登録

`OpenApiRouter` + `routes!` マクロで handler を登録すると、utoipa が path 情報を自動収集する。

```rust
let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
    .routes(routes!(routes::health::healthz))
    .routes(routes!(routes::plan::plan))
    .split_for_parts();
```

### カスタム extractor と utoipa の関係

utoipa のマクロは handler の引数型から OpenAPI のパラメータを推論しない（`axum_extras` feature による Path/Query の推論は除く）。`AuthenticatedToken` のようなカスタム extractor は OpenAPI ドキュメント上では透過的であり、`security` 属性で認証要件を明示する。

### ToSchema derive のポイント

- request/response に使う型は `#[derive(ToSchema)]` が必要
- ネストした構造体も全て `ToSchema` を derive する必要がある
- `serde` の rename, skip, flatten 属性は utoipa が認識する
- `Option<T>` は nullable として扱われる
- `Vec<T>` は array として扱われる
- `serde_json::Value` は `ToSchema` を自動実装している

## BoardFlow への示唆

Plan API の handler は以下の構成で実装可能:

1. `PlanRequest` / `PlanResponse` を `ToSchema` + `Serialize` + `Deserialize` で定義
2. `#[utoipa::path(post, path = "/api/v1/runs/plan", request_body = PlanRequest, ...)]` でマクロ指定
3. handler 引数は `(auth: AuthenticatedToken, Json(req): Json<PlanRequest>)` の順序
4. `security(("bearer_auth" = []))` で認証要件を明示
5. `OpenApiRouter` の `.routes(routes!(plan))` でルーティング登録

## 採用/不採用判断

**採用**: 上記パターンで Plan API のPOSTエンドポイントを実装する。既存の healthz handler パターンと一貫性がある。

## 制約と pitfall

- `request_body` に指定する型と handler の `Json<T>` の `T` は手動で一致させる必要がある（コンパイラは不一致を検出しない）
- ネストした全ての型に `ToSchema` を derive し忘れるとコンパイルエラーになる
- `axum_extras` feature は Path/Query パラメータの自動推論用であり、`Json` request body には関係しない
- `security` 属性を忘れると OpenAPI spec 上で認証不要に見えるが、実際は extractor で弾かれる（spec と実装の乖離）

## 未解決の疑問

なし。公式ドキュメントと既存 examples で十分に確認できた。

## 参照URL

- https://docs.rs/utoipa/5/utoipa/attr.path.html （path マクロ公式ドキュメント）
- https://docs.rs/utoipa-axum/0.2/utoipa_axum/ （utoipa-axum 公式ドキュメント）
- https://github.com/juhaku/utoipa/blob/master/examples/axum-utoipa-bindings/src/main.rs （公式 example）
- https://github.com/juhaku/utoipa/issues/891 （request_body の指定方法に関する issue）
