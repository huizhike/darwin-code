use super::*;
use pretty_assertions::assert_eq;

fn expected_minimal_provider(name: &str, base_url: &str) -> ModelProviderInfo {
    ModelProviderInfo {
        name: name.into(),
        base_url: Some(base_url.into()),
        api_key: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
    }
}

#[test]
fn test_deserialize_local_openai_compatible_model_provider_toml() {
    let provider_toml = r#"
name = "Local OpenAI Compatible"
base_url = "http://localhost:11434/v1"
        "#;
    let expected_provider =
        expected_minimal_provider("Local OpenAI Compatible", "http://localhost:11434/v1");

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
}

#[test]
fn test_deserialize_azure_model_provider_toml() {
    let provider_toml = r#"
name = "Azure"
base_url = "https://xxxxx.openai.azure.com/openai"
api_key = "azure-direct-key"
query_params = { api-version = "2025-04-01-preview" }
        "#;
    let expected_provider = ModelProviderInfo {
        name: "Azure".into(),
        base_url: Some("https://xxxxx.openai.azure.com/openai".into()),
        api_key: Some(InlineApiKey::new("azure-direct-key".into())),
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: Some(maplit::hashmap! {
            "api-version".to_string() => "2025-04-01-preview".to_string(),
        }),
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
    };

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
    assert_eq!(
        provider.api_key().expect("direct key should resolve"),
        Some("azure-direct-key".to_string())
    );
}

#[test]
fn test_deserialize_example_model_provider_toml() {
    let provider_toml = r#"
name = "Example"
base_url = "https://example.com"
api_key = "example-direct-key"
http_headers = { "X-Example-Header" = "example-value" }
env_http_headers = { "X-Example-Env-Header" = "EXAMPLE_ENV_VAR" }
        "#;
    let expected_provider = ModelProviderInfo {
        name: "Example".into(),
        base_url: Some("https://example.com".into()),
        api_key: Some(InlineApiKey::new("example-direct-key".into())),
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: Some(maplit::hashmap! {
            "X-Example-Header".to_string() => "example-value".to_string(),
        }),
        env_http_headers: Some(maplit::hashmap! {
            "X-Example-Env-Header".to_string() => "EXAMPLE_ENV_VAR".to_string(),
        }),
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
    };

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    assert_eq!(expected_provider, provider);
}

#[test]
fn test_deserialize_chat_wire_api_shows_helpful_error() {
    let provider_toml = r#"
name = "OpenAI using Chat Completions"
base_url = "https://api.openai.com/v1"
api_key = "direct-key"
wire_api = "chat"
        "#;

    let err = toml::from_str::<ModelProviderInfo>(provider_toml).unwrap_err();
    assert!(err.to_string().contains(CHAT_WIRE_API_REMOVED_ERROR));
}

#[test]
fn test_deserialize_chat_completions_wire_api() {
    let provider_toml = r#"
name = "DeepSeek"
base_url = "https://api.deepseek.com"
api_key = "direct-key"
wire_api = "chat-completions"
        "#;

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    assert_eq!(provider.wire_api, WireApi::ChatCompletions);
    assert_eq!(provider.wire_api.to_string(), "chat-completions");
}

#[test]
fn test_builtin_openai_provider_is_byok_http_sse() {
    let provider =
        ModelProviderInfo::create_openai_provider(/*base_url*/ None, /*api_key*/ None);

    assert!(provider.supports_remote_compaction());
    assert_eq!(
        provider.base_url.as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(provider.api_key().expect("direct key read"), None);
}

#[test]
fn test_create_openai_compatible_provider_uses_direct_api_key() {
    let provider = ModelProviderInfo::create_openai_compatible_provider(
        "DeepSeek".to_string(),
        "https://api.deepseek.com".to_string(),
        WireApi::ChatCompletions,
        Some("test-direct-api-key".to_string()),
    );

    assert_eq!(
        provider,
        ModelProviderInfo {
            name: "DeepSeek".to_string(),
            base_url: Some("https://api.deepseek.com".to_string()),
            api_key: Some(InlineApiKey::new("test-direct-api-key".to_string())),
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::ChatCompletions,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
        }
    );
    assert_eq!(
        provider.api_key().expect("direct api_key should resolve"),
        Some("test-direct-api-key".to_string())
    );
}

#[test]
fn test_openai_compatible_provider_direct_api_key_is_redacted() {
    let provider = ModelProviderInfo::create_openai_compatible_provider(
        "DeepSeek".to_string(),
        "https://api.deepseek.com".to_string(),
        WireApi::ChatCompletions,
        Some("test-direct-api-key".to_string()),
    );

    assert_eq!(
        provider.api_key().expect("direct api_key should resolve"),
        Some("test-direct-api-key".to_string())
    );
    assert!(
        !format!("{provider:?}").contains("test-direct-api-key"),
        "direct key should be redacted from debug output"
    );
}

#[test]
fn test_provider_without_direct_api_key_returns_none() {
    let provider = ModelProviderInfo::create_openai_compatible_provider(
        "DeepSeek".to_string(),
        "https://api.deepseek.com".to_string(),
        WireApi::ChatCompletions,
        None,
    );

    assert_eq!(provider.api_key().expect("direct key read"), None);
}

#[test]
fn test_legacy_env_backed_key_field_is_rejected() {
    let removed_field = format!("env_{}", "key");
    let provider_toml = format!(
        r#"
name = "Advanced"
base_url = "https://example.test/v1"
{removed_field} = "OLD_API_KEY"
        "#
    );

    let err = toml::from_str::<ModelProviderInfo>(&provider_toml).unwrap_err();
    assert!(err.to_string().contains(&removed_field));
}

#[test]
fn test_supports_remote_compaction_for_azure_name() {
    let provider = ModelProviderInfo {
        name: "Azure".into(),
        base_url: Some("https://example.com/openai".into()),
        api_key: Some(InlineApiKey::new("azure-direct-key".into())),
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
    };

    assert!(provider.supports_remote_compaction());
}

#[test]
fn test_supports_remote_compaction_for_non_openai_non_azure_provider() {
    let provider = ModelProviderInfo {
        name: "Example".into(),
        base_url: Some("https://example.com/v1".into()),
        api_key: Some(InlineApiKey::new("example-direct-key".into())),
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
    };

    assert!(!provider.supports_remote_compaction());
}

#[test]
fn test_deserialize_provider_auth_config_is_rejected() {
    let provider_toml = r#"
name = "Corp"

[auth]
command = "./scripts/print-token"
        "#;

    let provider: ModelProviderInfo = toml::from_str(provider_toml).unwrap();
    let err = provider
        .validate()
        .expect_err("command-backed provider auth is outside BYOK scope");

    assert!(
        err.contains("command-backed auth"),
        "unexpected error: {err}"
    );
}
