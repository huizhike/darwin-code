use std::sync::Arc;

use darwin_code_api::{AuthProvider, SharedAuthProvider};
use darwin_code_model_provider_info::{ModelProviderInfo, WireApi};
use http::{HeaderMap, HeaderName, HeaderValue};

use crate::bearer_auth_provider::BearerAuthProvider;

#[derive(Clone)]
struct HeaderAuthProvider {
    header_name: HeaderName,
    token: String,
}

impl AuthProvider for HeaderAuthProvider {
    fn add_auth_headers(&self, headers: &mut HeaderMap) {
        if let Ok(header) = HeaderValue::from_str(&self.token) {
            let _ = headers.insert(self.header_name.clone(), header);
        }
    }
}

pub(crate) fn resolve_provider_auth(
    provider: &ModelProviderInfo,
) -> darwin_code_protocol::error::Result<SharedAuthProvider> {
    let api_key = provider.api_key()?;
    match provider.wire_api {
        WireApi::Responses | WireApi::ChatCompletions => {
            Ok(Arc::new(BearerAuthProvider { token: api_key }))
        }
        WireApi::GeminiGenerate => {
            if let Some(token) = api_key {
                Ok(Arc::new(HeaderAuthProvider {
                    header_name: HeaderName::from_static("x-goog-api-key"),
                    token,
                }))
            } else {
                Ok(Arc::new(BearerAuthProvider { token: None }))
            }
        }
        WireApi::ClaudeMessages => {
            if let Some(token) = api_key {
                Ok(Arc::new(HeaderAuthProvider {
                    header_name: HeaderName::from_static("x-api-key"),
                    token,
                }))
            } else {
                Ok(Arc::new(BearerAuthProvider { token: None }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn auth_resolves_bearer_for_openai_compatible() {
        let provider = ModelProviderInfo::create_openai_compatible_provider(
            "Test BYOK".to_string(),
            "https://example.test/v1".to_string(),
            WireApi::Responses,
            Some("test-direct-key".to_string()),
        );

        let auth_provider = resolve_provider_auth(&provider).expect("auth should resolve");
        let mut headers = http::HeaderMap::new();
        auth_provider.add_auth_headers(&mut headers);

        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-direct-key")
        );
    }

    #[test]
    fn auth_resolves_bearer_for_chat_completions() {
        let provider = ModelProviderInfo::create_openai_compatible_provider(
            "DeepSeek".to_string(),
            "https://api.deepseek.com".to_string(),
            WireApi::ChatCompletions,
            Some("test-direct-key".to_string()),
        );

        let auth_provider = resolve_provider_auth(&provider).expect("auth should resolve");
        let mut headers = http::HeaderMap::new();
        auth_provider.add_auth_headers(&mut headers);

        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-direct-key")
        );
    }

    #[test]
    fn auth_resolves_header_for_gemini() {
        let provider = ModelProviderInfo::create_gemini_native_provider(
            None,
            Some("test-direct-key".to_string()),
        );

        let auth_provider = resolve_provider_auth(&provider).expect("auth should resolve");
        let mut headers = http::HeaderMap::new();
        auth_provider.add_auth_headers(&mut headers);

        assert_eq!(
            headers
                .get("x-goog-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("test-direct-key")
        );
    }

    #[test]
    fn auth_resolves_header_for_claude() {
        let provider = ModelProviderInfo::create_claude_native_provider(
            None,
            Some("test-direct-key".to_string()),
        );

        let auth_provider = resolve_provider_auth(&provider).expect("auth should resolve");
        let mut headers = http::HeaderMap::new();
        auth_provider.add_auth_headers(&mut headers);

        assert_eq!(
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("test-direct-key")
        );
    }

    #[test]
    fn auth_ignores_provider_without_api_key() {
        let provider = ModelProviderInfo::create_openai_compatible_provider(
            "Test BYOK".to_string(),
            "https://api.openai.com/v1".to_string(),
            WireApi::Responses,
            None,
        );

        let auth_provider = resolve_provider_auth(&provider).expect("auth should resolve");
        let mut headers = http::HeaderMap::new();
        auth_provider.add_auth_headers(&mut headers);

        assert!(headers.get(http::header::AUTHORIZATION).is_none());
    }
}
