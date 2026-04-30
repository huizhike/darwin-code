use super::*;

#[test]
fn deserialize_rejects_inline_bearer_token_field() {
    let err = toml::from_str::<McpServerConfig>(
        r#"
            url = "https://example.com"
            bearer_token = "secret"
        "#,
    )
    .expect_err("should reject bearer_token field");

    assert!(
        err.to_string().contains("bearer_token is not supported"),
        "unexpected error: {err}"
    );
}
