use insta::assert_snapshot;

/// OpenAPIスキーマのスナップショットテスト。
/// APIの型やエンドポイント定義が意図せず変更された場合にCIで検出する。
#[test]
fn test_openapi_schema_snapshot() {
    let schema = boardflow_api::openapi_schema();
    let json = serde_json::to_string_pretty(&schema).unwrap();
    assert_snapshot!("openapi_schema", json);
}
