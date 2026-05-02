//! Registry of model providers supported by DarwinCode.
//!
//! Providers can be defined in two places:
//!   1. Built-in defaults compiled into the binary so DarwinCode works out-of-the-box.
//!   2. User-defined entries inside `~/.darwin_code/config.toml` under the `model_providers`
//!      key. These override or extend the defaults at runtime.

use darwin_code_api::Provider as ApiProvider;
use darwin_code_api::RetryConfig as ApiRetryConfig;
use darwin_code_api::is_azure_responses_provider;
use darwin_code_protocol::config_types::ModelProviderAuthInfo;
use darwin_code_protocol::error::DarwinCodeErr;
use darwin_code_protocol::error::EnvVarError;
use darwin_code_protocol::error::Result as DarwinCodeResult;
use http::HeaderMap;
use http::header::HeaderName;
use http::header::HeaderValue;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_STREAM_MAX_RETRIES: u64 = 5;
const DEFAULT_REQUEST_MAX_RETRIES: u64 = 4;
/// Hard cap for user-configured `stream_max_retries`.
const MAX_STREAM_MAX_RETRIES: u64 = 100;
/// Hard cap for user-configured `request_max_retries`.
const MAX_REQUEST_MAX_RETRIES: u64 = 100;

const OPENAI_PROVIDER_NAME: &str = "OpenAI";
pub const OPENAI_PROVIDER_ID: &str = "openai";
pub const OPENAI_COMPATIBLE_PROVIDER_ID: &str = "openai-compatible";
const CHAT_WIRE_API_REMOVED_ERROR: &str = "`wire_api = \"chat\"` is ambiguous and no longer supported.\nHow to fix: set `wire_api = \"chat-completions\"` for Chat Completions providers, or `wire_api = \"responses\"` for Responses-compatible providers.";

/// Wire protocol that the provider speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WireApi {
    /// The Responses API exposed by OpenAI at `/v1/responses` and OpenAI-compatible endpoints.
    #[default]
    Responses,
    /// The Chat Completions API exposed by OpenAI-compatible providers at `/chat/completions`.
    ChatCompletions,
    /// The Gemini API exposed by Google.
    GeminiGenerate,
    /// The Claude API exposed by Anthropic.
    ClaudeMessages,
}

impl fmt::Display for WireApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Responses => "responses",
            Self::ChatCompletions => "chat-completions",
            Self::GeminiGenerate => "gemini-generate",
            Self::ClaudeMessages => "claude-messages",
        };
        f.write_str(value)
    }
}

impl<'de> Deserialize<'de> for WireApi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "responses" => Ok(Self::Responses),
            "chat-completions" => Ok(Self::ChatCompletions),
            "gemini-generate" => Ok(Self::GeminiGenerate),
            "claude-messages" => Ok(Self::ClaudeMessages),
            "chat" => Err(serde::de::Error::custom(CHAT_WIRE_API_REMOVED_ERROR)),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "responses",
                    "chat-completions",
                    "gemini-generate",
                    "claude-messages",
                ],
            )),
        }
    }
}

/// Inline BYOK API key for product provider configs.
///
/// Debug deliberately redacts the secret because `ModelProviderInfo` appears in
/// diagnostics and config assertions.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct InlineApiKey(String);

impl InlineApiKey {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for InlineApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Serializable representation of a provider definition.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderInfo {
    /// Friendly display name.
    pub name: String,
    /// Base URL for the provider's OpenAI-compatible API.
    pub base_url: Option<String>,
    /// Direct BYOK API key for this provider. The user supplies this in local
    /// config; Debug output redacts it and serialization skips it to avoid
    /// leaking secrets through status/config APIs.
    #[serde(default, skip_serializing)]
    pub api_key: Option<InlineApiKey>,
    /// Environment variable containing the BYOK API key.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Legacy bearer-token field retained only for config-schema compatibility.
    /// DarwinCode is BYOK-only; provider configs use `api_key_env` or direct
    /// `api_key`.
    pub experimental_bearer_token: Option<String>,
    /// Legacy command-backed bearer-token configuration. DarwinCode is
    /// BYOK-only; provider configs use `api_key_env` or direct `api_key`.
    pub auth: Option<ModelProviderAuthInfo>,
    /// Which wire protocol this provider expects.
    #[serde(default)]
    pub wire_api: WireApi,
    /// Optional query parameters to append to the base URL.
    pub query_params: Option<HashMap<String, String>>,
    /// Additional HTTP headers to include in requests to this provider where
    /// the (key, value) pairs are the header name and value.
    pub http_headers: Option<HashMap<String, String>>,
    /// Optional HTTP headers to include in requests to this provider where the
    /// (key, value) pairs are the header name and _environment variable_ whose
    /// value should be used. If the environment variable is not set, or the
    /// value is empty, the header will not be included in the request.
    pub env_http_headers: Option<HashMap<String, String>>,
    /// Maximum number of times to retry a failed HTTP request to this provider.
    pub request_max_retries: Option<u64>,
    /// Number of times to retry reconnecting a dropped streaming response before failing.
    pub stream_max_retries: Option<u64>,
    /// Idle timeout (in milliseconds) to wait for activity on a streaming response before treating
    /// the connection as lost.
    pub stream_idle_timeout_ms: Option<u64>,
}

impl ModelProviderInfo {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.api_key.is_some() && self.api_key_env.is_some() {
            return Err("cannot set both `api_key` and `api_key_env`".to_string());
        }
        if self
            .api_key_env
            .as_deref()
            .is_some_and(|env_var| env_var.trim().is_empty())
        {
            return Err("`api_key_env` must not be empty".to_string());
        }
        if self.experimental_bearer_token.is_some() {
            return Err(
                "BYOK-only provider auth rejects experimental_bearer_token; use api_key or api_key_env"
                    .to_string(),
            );
        }
        if self.auth.is_some() {
            return Err(
                "BYOK-only provider auth rejects command-backed auth; use api_key or api_key_env"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn build_header_map(&self) -> DarwinCodeResult<HeaderMap> {
        let capacity = self.http_headers.as_ref().map_or(0, HashMap::len)
            + self.env_http_headers.as_ref().map_or(0, HashMap::len);
        let mut headers = HeaderMap::with_capacity(capacity);
        if let Some(extra) = &self.http_headers {
            for (k, v) in extra {
                if let (Ok(name), Ok(value)) = (HeaderName::try_from(k), HeaderValue::try_from(v)) {
                    headers.insert(name, value);
                }
            }
        }

        if let Some(env_headers) = &self.env_http_headers {
            for (header, env_var) in env_headers {
                if let Ok(val) = std::env::var(env_var)
                    && !val.trim().is_empty()
                    && let (Ok(name), Ok(value)) =
                        (HeaderName::try_from(header), HeaderValue::try_from(val))
                {
                    headers.insert(name, value);
                }
            }
        }

        Ok(headers)
    }

    pub fn to_api_provider(&self) -> DarwinCodeResult<ApiProvider> {
        let base_url = self.base_url.clone().unwrap_or_default();

        let headers = self.build_header_map()?;
        let retry = ApiRetryConfig {
            max_attempts: self.request_max_retries(),
            base_delay: Duration::from_millis(200),
            retry_429: false,
            retry_5xx: true,
            retry_transport: true,
        };

        Ok(ApiProvider {
            name: self.name.clone(),
            base_url,
            query_params: self.query_params.clone(),
            headers,
            retry,
            stream_idle_timeout: self.stream_idle_timeout(),
        })
    }

    /// Returns the BYOK API key configured for this provider, if any.
    pub fn api_key(&self) -> DarwinCodeResult<Option<String>> {
        if let Some(api_key) = self.api_key.as_ref() {
            return Ok(Some(api_key.expose_secret().to_string()));
        }

        let Some(env_var) = self.api_key_env.as_deref() else {
            return Ok(None);
        };
        let env_var = env_var.trim();
        if env_var.is_empty() {
            return Err(DarwinCodeErr::Fatal(
                "`api_key_env` must not be empty".to_string(),
            ));
        }
        let value = std::env::var(env_var).map_err(|_| {
            DarwinCodeErr::EnvVar(EnvVarError {
                var: env_var.to_string(),
                instructions: Some("Set it or update `api_key_env`.".to_string()),
            })
        })?;
        if value.trim().is_empty() {
            return Err(DarwinCodeErr::Fatal(format!(
                "Environment variable `{env_var}` referenced by `api_key_env` must not be empty"
            )));
        }
        Ok(Some(value))
    }

    pub fn with_api_key(mut self, api_key: Option<String>) -> Self {
        self.api_key = api_key.map(InlineApiKey::new);
        self.api_key_env = None;
        self
    }

    /// Effective maximum number of request retries for this provider.
    pub fn request_max_retries(&self) -> u64 {
        self.request_max_retries
            .unwrap_or(DEFAULT_REQUEST_MAX_RETRIES)
            .min(MAX_REQUEST_MAX_RETRIES)
    }

    /// Effective maximum number of stream reconnection attempts for this provider.
    pub fn stream_max_retries(&self) -> u64 {
        self.stream_max_retries
            .unwrap_or(DEFAULT_STREAM_MAX_RETRIES)
            .min(MAX_STREAM_MAX_RETRIES)
    }

    /// Effective idle timeout for streaming responses.
    pub fn stream_idle_timeout(&self) -> Duration {
        self.stream_idle_timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_millis(DEFAULT_STREAM_IDLE_TIMEOUT_MS))
    }

    pub fn create_openai_provider(
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> ModelProviderInfo {
        ModelProviderInfo {
            name: OPENAI_PROVIDER_NAME.into(),
            base_url: Some(base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string())),
            api_key: api_key.map(InlineApiKey::new),
            api_key_env: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::Responses,
            query_params: None,
            http_headers: Some(
                [("version".to_string(), env!("CARGO_PKG_VERSION").to_string())]
                    .into_iter()
                    .collect(),
            ),
            env_http_headers: Some(
                [
                    (
                        "OpenAI-Organization".to_string(),
                        "OPENAI_ORGANIZATION".to_string(),
                    ),
                    ("OpenAI-Project".to_string(), "OPENAI_PROJECT".to_string()),
                ]
                .into_iter()
                .collect(),
            ),
            // Use global defaults for retry/timeout unless overridden in config.toml.
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
        }
    }

    pub fn create_gemini_native_provider(
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> ModelProviderInfo {
        ModelProviderInfo {
            name: "Google Gemini".into(),
            base_url: Some(
                base_url.unwrap_or_else(|| {
                    "https://generativelanguage.googleapis.com/v1beta".to_string()
                }),
            ),
            api_key: api_key.map(InlineApiKey::new),
            api_key_env: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::GeminiGenerate,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
        }
    }

    pub fn create_claude_native_provider(
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> ModelProviderInfo {
        ModelProviderInfo {
            name: "Anthropic Claude".into(),
            base_url: Some(base_url.unwrap_or_else(|| "https://api.anthropic.com/v1".to_string())),
            api_key: api_key.map(InlineApiKey::new),
            api_key_env: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api: WireApi::ClaudeMessages,
            query_params: None,
            http_headers: Some(
                [("anthropic-version".to_string(), "2023-06-01".to_string())]
                    .into_iter()
                    .collect(),
            ),
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
        }
    }

    pub fn create_openai_compatible_provider(
        name: String,
        base_url: String,
        wire_api: WireApi,
        api_key: impl Into<Option<String>>,
    ) -> ModelProviderInfo {
        ModelProviderInfo {
            name,
            base_url: Some(base_url),
            api_key: api_key.into().map(InlineApiKey::new),
            api_key_env: None,
            experimental_bearer_token: None,
            auth: None,
            wire_api,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: None,
            stream_max_retries: None,
            stream_idle_timeout_ms: None,
        }
    }

    pub fn is_openai(&self) -> bool {
        self.name == OPENAI_PROVIDER_NAME
    }

    pub fn supports_remote_compaction(&self) -> bool {
        self.is_openai() || is_azure_responses_provider(&self.name, self.base_url.as_deref())
    }

    pub fn has_command_auth(&self) -> bool {
        self.auth.is_some()
    }
}

#[cfg(test)]
#[path = "model_provider_info_tests.rs"]
mod tests;
