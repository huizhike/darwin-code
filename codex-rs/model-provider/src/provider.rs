use std::fmt;
use std::sync::Arc;

use darwin_code_api::Provider;
use darwin_code_api::SharedAuthProvider;
use darwin_code_model_provider_info::ModelProviderInfo;

use crate::auth::resolve_provider_auth;

/// Runtime provider abstraction used by model execution.
///
/// Implementations own provider-specific behavior for a model backend. The
/// `ModelProviderInfo` returned by `info` is the serialized/configured provider
/// metadata used by the default OpenAI-compatible implementation.
#[async_trait::async_trait]
pub trait ModelProvider: fmt::Debug + Send + Sync {
    /// Returns the configured provider metadata.
    fn info(&self) -> &ModelProviderInfo;

    /// Returns provider configuration adapted for the API client.
    async fn api_provider(&self) -> darwin_code_protocol::error::Result<Provider> {
        self.info().to_api_provider()
    }

    /// Returns the auth provider used to attach request credentials.
    async fn api_auth(&self) -> darwin_code_protocol::error::Result<SharedAuthProvider> {
        resolve_provider_auth(self.info())
    }
}

/// Shared runtime model provider handle.
pub type SharedModelProvider = Arc<dyn ModelProvider>;

/// Creates the default runtime model provider for configured provider metadata.
///
/// Provider credentials are resolved only from `ModelProviderInfo`.
pub fn create_model_provider(provider_info: ModelProviderInfo) -> SharedModelProvider {
    Arc::new(ConfiguredModelProvider {
        info: provider_info,
    })
}

/// Runtime model provider backed by configured `ModelProviderInfo`.
#[derive(Clone, Debug)]
struct ConfiguredModelProvider {
    info: ModelProviderInfo,
}

#[async_trait::async_trait]
impl ModelProvider for ConfiguredModelProvider {
    fn info(&self) -> &ModelProviderInfo {
        &self.info
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use darwin_code_model_provider_info::WireApi;
    use darwin_code_protocol::config_types::ModelProviderAuthInfo;

    use super::*;

    fn provider_info_with_command_auth() -> ModelProviderInfo {
        ModelProviderInfo {
            auth: Some(ModelProviderAuthInfo {
                command: "print-token".to_string(),
                args: Vec::new(),
                timeout_ms: NonZeroU64::new(5_000).expect("timeout should be non-zero"),
                refresh_interval_ms: 300_000,
                cwd: std::env::current_dir()
                    .expect("current dir should be available")
                    .try_into()
                    .expect("current dir should be absolute"),
            }),
            ..ModelProviderInfo::create_openai_compatible_provider(
                "Test BYOK".to_string(),
                "https://api.openai.com/v1".to_string(),
                WireApi::Responses,
                "OPENAI_API_KEY".to_string(),
            )
        }
    }

    #[test]
    fn create_model_provider_preserves_configured_auth_metadata() {
        let provider = create_model_provider(provider_info_with_command_auth());

        assert!(provider.info().auth.is_some());
    }

    #[tokio::test]
    async fn api_provider_uses_configured_byok_boundary() {
        let provider = create_model_provider(ModelProviderInfo::create_openai_compatible_provider(
            "Test BYOK".to_string(),
            "https://api.openai.com/v1".to_string(),
            WireApi::Responses,
            "OPENAI_API_KEY".to_string(),
        ));

        let api_provider = provider
            .api_provider()
            .await
            .expect("provider config should resolve");

        assert_eq!(api_provider.base_url, "https://api.openai.com/v1");
    }
}
