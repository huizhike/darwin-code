//! Session- and turn-scoped helpers for talking to model provider APIs.
//!
//! `ModelClient` is intended to live for the lifetime of a DarwinCode session and holds only stable
//! request identity and transport state. Provider routing is turn-scoped: callers pass the provider
//! resolved from the current model into every request.
//!
//! Per-turn settings (model selection, reasoning controls, and turn metadata)
//! are passed explicitly to streaming and unary methods so that the turn lifetime is visible at the
//! call site.
//!
//! A [`ModelClientSession`] is created per turn and is used to stream one or more HTTP/SSE
//! requests during that turn. It stores per-turn state such as the `x-darwin_code-turn-state`
//! token used for sticky routing.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use darwin_code_api::ApiError;
use darwin_code_api::ChatCompletionsApiRequest;
use darwin_code_api::ChatCompletionsClient as ApiChatCompletionsClient;
use darwin_code_api::ChatCompletionsFunctionCall;
use darwin_code_api::ChatCompletionsMessage;
use darwin_code_api::ChatCompletionsOptions as ApiChatCompletionsOptions;
use darwin_code_api::ChatCompletionsToolCall;
use darwin_code_api::CompactClient as ApiCompactClient;
use darwin_code_api::CompactionInput as ApiCompactionInput;
use darwin_code_api::Compression;
use darwin_code_api::MemoriesClient as ApiMemoriesClient;
use darwin_code_api::MemorySummarizeInput as ApiMemorySummarizeInput;
use darwin_code_api::MemorySummarizeOutput as ApiMemorySummarizeOutput;
use darwin_code_api::Provider as ApiProvider;
use darwin_code_api::RawMemory as ApiRawMemory;
use darwin_code_api::Reasoning;
use darwin_code_api::ReqwestTransport;
use darwin_code_api::ResponsesApiRequest;
use darwin_code_api::ResponsesClient as ApiResponsesClient;
use darwin_code_api::ResponsesOptions as ApiResponsesOptions;
use darwin_code_api::SharedAuthProvider;
use darwin_code_api::TransportError;
use darwin_code_api::build_conversation_headers;
use darwin_code_api::create_text_param_for_request;
use darwin_code_otel::SessionTelemetry;
use darwin_code_response_debug_context::extract_response_debug_context;

use darwin_code_protocol::ThreadId;
use darwin_code_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use darwin_code_protocol::config_types::ServiceTier;
use darwin_code_protocol::config_types::Verbosity as VerbosityConfig;
use darwin_code_protocol::model_metadata::ModelInfo;
use darwin_code_protocol::model_metadata::ReasoningEffort as ReasoningEffortConfig;
use darwin_code_protocol::models::ContentItem;
use darwin_code_protocol::models::FunctionCallOutputBody;
use darwin_code_protocol::models::LocalShellAction;
use darwin_code_protocol::models::ResponseItem;
use darwin_code_protocol::protocol::SessionSource;
use darwin_code_protocol::protocol::SubAgentSource;
use darwin_code_tools::ToolSpec;
use darwin_code_tools::create_tools_json_for_responses_api;
use futures::StreamExt;
use http::HeaderMap as ApiHeaderMap;
use http::HeaderValue;
use reqwest::StatusCode;
use tokio::sync::mpsc;
use tracing::instrument;
use tracing::trace;
use tracing::warn;

use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::client_common::ResponseStream;
use crate::flags::DARWIN_CODE_RS_SSE_FIXTURE;
use crate::util::emit_feedback_auth_recovery_tags;
use darwin_code_api::map_api_error;
use darwin_code_model_provider::SharedModelProvider;
use darwin_code_model_provider_info::WireApi;
use darwin_code_protocol::error::Result;

pub const X_DARWIN_CODE_INSTALLATION_ID_HEADER: &str = "x-darwin_code-installation-id";
pub const X_DARWIN_CODE_TURN_STATE_HEADER: &str = "x-darwin_code-turn-state";
pub const X_DARWIN_CODE_TURN_METADATA_HEADER: &str = "x-darwin_code-turn-metadata";
pub const X_DARWIN_CODE_PARENT_THREAD_ID_HEADER: &str = "x-darwin_code-parent-thread-id";
pub const X_DARWIN_CODE_WINDOW_ID_HEADER: &str = "x-darwin_code-window-id";
pub const X_DARWIN_SUBAGENT_HEADER: &str = "x-darwin-subagent";
pub const X_RESPONSESAPI_INCLUDE_TIMING_METRICS_HEADER: &str =
    "x-responsesapi-include-timing-metrics";

/// Session-scoped state shared by all [`ModelClient`] clones.
///
/// This is intentionally kept minimal so `ModelClient` does not need to hold a full `Config`. Most
/// configuration is per turn and is passed explicitly to streaming/unary methods.
#[derive(Debug)]
struct ModelClientState {
    conversation_id: ThreadId,
    window_generation: AtomicU64,
    installation_id: String,
    session_source: SessionSource,
    model_verbosity: Option<VerbosityConfig>,
    #[allow(dead_code)]
    enable_request_compression: bool,
    beta_features_header: Option<String>,
}

/// Resolved API client setup for a single request attempt.
///
/// Keeping this as a single bundle ensures prewarm and normal request paths
/// share the same auth/provider setup flow.
struct CurrentClientSetup {
    api_provider: ApiProvider,
    api_auth: SharedAuthProvider,
}

/// A session-scoped client for model-provider API calls.
///
/// This holds configuration and state that should be shared across turns within a DarwinCode session
/// (conversation id, request identity headers, and transport fallback state).
///
/// Turn-scoped settings (model selection, reasoning controls, turn metadata) are passed explicitly to the relevant methods to keep turn lifetime visible at the
/// call site.
#[derive(Debug, Clone)]
pub struct ModelClient {
    state: Arc<ModelClientState>,
}

/// A turn-scoped streaming session created from a [`ModelClient`].
///
/// The session caches per-turn state:
///
/// - The `x-darwin_code-turn-state` sticky-routing token, which must be replayed for all requests within
///   the same turn.
///
/// Create a fresh `ModelClientSession` for each DarwinCode turn. Reusing it across turns would replay
/// the previous turn's sticky-routing token into the next turn, which violates the client/server
/// contract and can cause routing bugs.
pub struct ModelClientSession {
    client: ModelClient,
    /// Turn state for sticky routing.
    ///
    /// This is an `OnceLock` that stores the turn state value received from the server
    /// on turn start via the `x-darwin_code-turn-state` response header. Once set, this value
    /// should be sent back to the server in the `x-darwin_code-turn-state` request header for
    /// all subsequent requests within the same turn to maintain sticky routing.
    ///
    /// This is a contract between the client and server: we receive it at turn start,
    /// keep sending it unchanged between turn requests (e.g., for retries, incremental
    /// appends, or continuation requests), and must not send it between different turns.
    turn_state: Arc<OnceLock<String>>,
}

impl ModelClient {
    #[allow(clippy::too_many_arguments)]
    /// Creates a new session-scoped `ModelClient`.
    ///
    /// All arguments are expected to be stable for the lifetime of a DarwinCode session. Per-turn values,
    /// including provider routing, are passed to [`ModelClientSession::stream`] and related request
    /// methods explicitly.
    pub fn new(
        conversation_id: ThreadId,
        installation_id: String,
        session_source: SessionSource,
        model_verbosity: Option<VerbosityConfig>,
        #[allow(dead_code)] enable_request_compression: bool,
        _include_timing_metrics: bool,
        beta_features_header: Option<String>,
    ) -> Self {
        Self {
            state: Arc::new(ModelClientState {
                conversation_id,
                window_generation: AtomicU64::new(0),
                installation_id,
                session_source,
                model_verbosity,
                enable_request_compression,
                beta_features_header,
            }),
        }
    }

    /// Creates a fresh turn-scoped streaming session.
    ///
    /// This constructor does not perform network I/O itself.
    pub fn new_session(&self) -> ModelClientSession {
        ModelClientSession {
            client: self.clone(),
            turn_state: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn set_window_generation(&self, window_generation: u64) {
        self.state
            .window_generation
            .store(window_generation, Ordering::Relaxed);
    }

    pub(crate) fn advance_window_generation(&self) {
        self.state.window_generation.fetch_add(1, Ordering::Relaxed);
    }

    fn current_window_id(&self) -> String {
        let conversation_id = self.state.conversation_id;
        let window_generation = self.state.window_generation.load(Ordering::Relaxed);
        format!("{conversation_id}:{window_generation}")
    }
    /// Compacts the current conversation history using the Compact endpoint.
    ///
    /// This is a unary call (no streaming) that returns a new list of
    /// `ResponseItem`s representing the compacted transcript.
    ///
    /// The provider and model selection are passed explicitly to keep request routing turn-scoped.
    pub async fn compact_conversation_history(
        &self,
        provider: &SharedModelProvider,
        prompt: &Prompt,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
    ) -> Result<Vec<ResponseItem>> {
        if prompt.input.is_empty() {
            return Ok(Vec::new());
        }
        let client_setup = current_client_setup(provider).await?;
        let transport = ReqwestTransport::new(darwin_code_client::build_reqwest_client());
        let client =
            ApiCompactClient::new(transport, client_setup.api_provider, client_setup.api_auth);

        let instructions = prompt.base_instructions.text.clone();
        let input = prompt.get_formatted_input();
        let tools = create_tools_json_for_responses_api(&prompt.tools)?;
        let reasoning = Self::build_reasoning(model_info, effort, summary);
        let verbosity = if model_info.support_verbosity {
            self.state.model_verbosity.or(model_info.default_verbosity)
        } else {
            if self.state.model_verbosity.is_some() {
                warn!(
                    "model_verbosity is set but ignored as the model does not support verbosity: {}",
                    model_info.slug
                );
            }
            None
        };
        let text = create_text_param_for_request(verbosity, &prompt.output_schema);
        let payload = ApiCompactionInput {
            model: &model_info.slug,
            input: &input,
            instructions: &instructions,
            tools,
            parallel_tool_calls: prompt.parallel_tool_calls,
            reasoning,
            text,
        };

        let mut extra_headers = ApiHeaderMap::new();
        if let Ok(header_value) = HeaderValue::from_str(&self.state.installation_id) {
            extra_headers.insert(X_DARWIN_CODE_INSTALLATION_ID_HEADER, header_value);
        }
        extra_headers.extend(self.build_responses_identity_headers());
        extra_headers.extend(build_conversation_headers(Some(
            self.state.conversation_id.to_string(),
        )));
        client
            .compact_input(&payload, extra_headers)
            .await
            .map_err(map_api_error)
    }

    /// Builds memory summaries for each provided normalized raw memory.
    ///
    /// This is a unary call (no streaming) to `/v1/memories/trace_summarize`.
    ///
    /// The provider, model selection, and reasoning effort are passed explicitly to keep request routing
    /// turn-scoped.
    pub async fn summarize_memories(
        &self,
        provider: &SharedModelProvider,
        raw_memories: Vec<ApiRawMemory>,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
    ) -> Result<Vec<ApiMemorySummarizeOutput>> {
        if raw_memories.is_empty() {
            return Ok(Vec::new());
        }

        let client_setup = current_client_setup(provider).await?;
        let transport = ReqwestTransport::new(darwin_code_client::build_reqwest_client());
        let client =
            ApiMemoriesClient::new(transport, client_setup.api_provider, client_setup.api_auth);

        let payload = ApiMemorySummarizeInput {
            model: model_info.slug.clone(),
            raw_memories,
            reasoning: effort.map(|effort| Reasoning {
                effort: Some(effort),
                summary: None,
            }),
        };

        client
            .summarize_input(&payload, self.build_subagent_headers())
            .await
            .map_err(map_api_error)
    }

    fn build_subagent_headers(&self) -> ApiHeaderMap {
        let mut extra_headers = ApiHeaderMap::new();
        if let Some(subagent) = subagent_header_value(&self.state.session_source)
            && let Ok(val) = HeaderValue::from_str(&subagent)
        {
            extra_headers.insert(X_DARWIN_SUBAGENT_HEADER, val);
        }
        extra_headers
    }

    fn build_responses_identity_headers(&self) -> ApiHeaderMap {
        let mut extra_headers = self.build_subagent_headers();
        if let Some(parent_thread_id) = parent_thread_id_header_value(&self.state.session_source)
            && let Ok(val) = HeaderValue::from_str(&parent_thread_id)
        {
            extra_headers.insert(X_DARWIN_CODE_PARENT_THREAD_ID_HEADER, val);
        }
        if let Ok(val) = HeaderValue::from_str(&self.current_window_id()) {
            extra_headers.insert(X_DARWIN_CODE_WINDOW_ID_HEADER, val);
        }
        extra_headers
    }

    fn build_reasoning(
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
    ) -> Option<Reasoning> {
        if model_info.supports_reasoning_summaries {
            Some(Reasoning {
                effort: effort.or(model_info.default_reasoning_level),
                summary: if summary == ReasoningSummaryConfig::None {
                    None
                } else {
                    Some(summary)
                },
            })
        } else {
            None
        }
    }
}

/// Returns auth + provider configuration for the current turn provider.
///
/// This centralizes setup used by both prewarm and normal request paths so they stay in
/// lockstep while still taking provider routing from the current model selection.
async fn current_client_setup(provider: &SharedModelProvider) -> Result<CurrentClientSetup> {
    let api_provider = provider.api_provider().await?;
    let api_auth = provider.api_auth().await?;
    Ok(CurrentClientSetup {
        api_provider,
        api_auth,
    })
}

impl ModelClientSession {
    fn build_responses_request(
        &self,
        provider: &darwin_code_api::Provider,
        prompt: &Prompt,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
    ) -> Result<ResponsesApiRequest> {
        let instructions = &prompt.base_instructions.text;
        let input = prompt.get_formatted_input();
        let tools = create_tools_json_for_responses_api(&prompt.tools)?;
        let default_reasoning_effort = model_info.default_reasoning_level;
        let reasoning = if model_info.supports_reasoning_summaries {
            Some(Reasoning {
                effort: effort.or(default_reasoning_effort),
                summary: if summary == ReasoningSummaryConfig::None {
                    None
                } else {
                    Some(summary)
                },
            })
        } else {
            None
        };
        let include = if reasoning.is_some() {
            vec!["reasoning.encrypted_content".to_string()]
        } else {
            Vec::new()
        };
        let verbosity = if model_info.support_verbosity {
            self.client
                .state
                .model_verbosity
                .or(model_info.default_verbosity)
        } else {
            if self.client.state.model_verbosity.is_some() {
                warn!(
                    "model_verbosity is set but ignored as the model does not support verbosity: {}",
                    model_info.slug
                );
            }
            None
        };
        let text = create_text_param_for_request(verbosity, &prompt.output_schema);
        let prompt_cache_key = Some(self.client.state.conversation_id.to_string());
        let request = ResponsesApiRequest {
            model: model_info.slug.clone(),
            instructions: instructions.clone(),
            input,
            tools,
            tool_choice: "auto".to_string(),
            parallel_tool_calls: prompt.parallel_tool_calls,
            reasoning,
            store: provider.is_azure_responses_endpoint(),
            stream: true,
            include,
            service_tier: match service_tier {
                Some(ServiceTier::Fast) => Some("priority".to_string()),
                Some(service_tier) => Some(service_tier.to_string()),
                None => None,
            },
            prompt_cache_key,
            text,
            client_metadata: Some(HashMap::from([(
                X_DARWIN_CODE_INSTALLATION_ID_HEADER.to_string(),
                self.client.state.installation_id.clone(),
            )])),
        };
        Ok(request)
    }

    fn build_chat_completions_request(
        &self,
        provider: &SharedModelProvider,
        prompt: &Prompt,
        model_info: &ModelInfo,
        effort: Option<ReasoningEffortConfig>,
    ) -> Result<ChatCompletionsApiRequest> {
        let messages = chat_messages_from_prompt(prompt);
        let tools = create_tools_json_for_chat_completions_api(&prompt.tools)?;
        let is_openai_compatible = provider.info().wire_api == WireApi::ChatCompletions;
        let reasoning_effort = if model_info.supports_reasoning_summaries && !is_openai_compatible {
            effort
                .or(model_info.default_reasoning_level)
                .filter(|effort| *effort != ReasoningEffortConfig::None)
        } else {
            None
        };
        let thinking = if model_info.supports_reasoning_summaries && is_openai_compatible {
            let effort = effort.or(model_info.default_reasoning_level);
            let effort_str = match effort {
                Some(ReasoningEffortConfig::Minimal) | Some(ReasoningEffortConfig::Low) => {
                    Some("low".to_string())
                }
                Some(ReasoningEffortConfig::Medium) => Some("medium".to_string()),
                Some(ReasoningEffortConfig::High) | Some(ReasoningEffortConfig::XHigh) => {
                    Some("high".to_string())
                }
                Some(ReasoningEffortConfig::None) | None => None,
            };
            Some(darwin_code_api::ChatCompletionsThinkingConfig {
                r#type: "enabled".to_string(),
                effort: effort_str,
            })
        } else {
            None
        };
        let request = ChatCompletionsApiRequest {
            model: model_info.slug.clone(),
            messages,
            tool_choice: (!tools.is_empty()).then(|| "auto".to_string()),
            parallel_tool_calls: (!tools.is_empty() && prompt.parallel_tool_calls).then_some(true),
            tools,
            stream: true,
            reasoning_effort,
            thinking,
        };
        Ok(request)
    }

    fn build_chat_completions_options(
        &self,
        turn_metadata_header: Option<&str>,
    ) -> ApiChatCompletionsOptions {
        let turn_metadata_header = parse_turn_metadata_header(turn_metadata_header);
        let conversation_id = self.client.state.conversation_id.to_string();
        ApiChatCompletionsOptions {
            conversation_id: Some(conversation_id),
            session_source: Some(self.client.state.session_source.clone()),
            extra_headers: {
                let mut headers = build_responses_headers(
                    self.client.state.beta_features_header.as_deref(),
                    Some(&self.turn_state),
                    turn_metadata_header.as_ref(),
                );
                headers.extend(self.client.build_responses_identity_headers());
                headers
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Builds shared Responses API transport options and request-body options.
    ///
    /// Keeping option construction in one place ensures request-scoped headers are consistent
    /// regardless of transport choice.
    fn build_responses_options(
        &self,
        turn_metadata_header: Option<&str>,
        compression: Compression,
    ) -> ApiResponsesOptions {
        let turn_metadata_header = parse_turn_metadata_header(turn_metadata_header);
        let conversation_id = self.client.state.conversation_id.to_string();
        ApiResponsesOptions {
            conversation_id: Some(conversation_id),
            session_source: Some(self.client.state.session_source.clone()),
            extra_headers: {
                let mut headers = build_responses_headers(
                    self.client.state.beta_features_header.as_deref(),
                    Some(&self.turn_state),
                    turn_metadata_header.as_ref(),
                );
                headers.extend(self.client.build_responses_identity_headers());
                headers
            },
            compression,
            turn_state: Some(Arc::clone(&self.turn_state)),
        }
    }
    fn responses_request_compression(&self) -> Compression {
        Compression::None
    }

    /// Streams a turn via an OpenAI-compatible Chat Completions API.
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        name = "model_client.stream_chat_completions_api",
        level = "info",
        skip_all,
        fields(
            model = %model_info.slug,
            wire_api = %provider.info().wire_api,
            transport = "chat_completions_http",
            http.method = "POST",
            api.path = "chat/completions",
            turn.has_metadata_header = turn_metadata_header.is_some()
        )
    )]
    async fn stream_chat_completions_api(
        &self,
        provider: &SharedModelProvider,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        turn_metadata_header: Option<&str>,
    ) -> Result<ResponseStream> {
        loop {
            let client_setup = current_client_setup(provider).await?;
            let transport = ReqwestTransport::new(darwin_code_client::build_reqwest_client());
            let options = self.build_chat_completions_options(turn_metadata_header);
            let request =
                self.build_chat_completions_request(provider, prompt, model_info, effort)?;
            let client = ApiChatCompletionsClient::new(
                transport,
                client_setup.api_provider,
                client_setup.api_auth,
            );
            let stream_result = client.stream_request(request, options).await;

            match stream_result {
                Ok(stream) => {
                    let stream = map_response_stream(stream, session_telemetry.clone());
                    return Ok(stream);
                }
                Err(ApiError::Transport(
                    unauthorized_transport @ TransportError::Http { status, .. },
                )) if status == StatusCode::UNAUTHORIZED => {
                    handle_unauthorized(unauthorized_transport, session_telemetry).await?;
                    continue;
                }
                Err(err) => return Err(map_api_error(err)),
            }
        }
    }

    /// Streams a turn via the OpenAI Responses API.
    ///
    /// Handles SSE fixtures, reasoning summaries, verbosity, and the
    /// `text` controls used for output schemas.
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        name = "model_client.stream_responses_api",
        level = "info",
        skip_all,
        fields(
            model = %model_info.slug,
            wire_api = %provider.info().wire_api,
            transport = "responses_http",
            http.method = "POST",
            api.path = "responses",
            turn.has_metadata_header = turn_metadata_header.is_some()
        )
    )]
    async fn stream_responses_api(
        &self,
        provider: &SharedModelProvider,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
        turn_metadata_header: Option<&str>,
    ) -> Result<ResponseStream> {
        if let Some(path) = &*DARWIN_CODE_RS_SSE_FIXTURE {
            warn!(path, "Streaming from fixture");
            let stream =
                darwin_code_api::stream_from_fixture(path, provider.info().stream_idle_timeout())
                    .map_err(map_api_error)?;
            let stream = map_response_stream(stream, session_telemetry.clone());
            return Ok(stream);
        }

        loop {
            let client_setup = current_client_setup(provider).await?;
            let transport = ReqwestTransport::new(darwin_code_client::build_reqwest_client());
            let compression = self.responses_request_compression();
            let options = self.build_responses_options(turn_metadata_header, compression);

            let request = self.build_responses_request(
                &client_setup.api_provider,
                prompt,
                model_info,
                effort,
                summary,
                service_tier,
            )?;
            let client = ApiResponsesClient::new(
                transport,
                client_setup.api_provider,
                client_setup.api_auth,
            );
            let stream_result = client.stream_request(request, options).await;

            match stream_result {
                Ok(stream) => {
                    let stream = map_response_stream(stream, session_telemetry.clone());
                    return Ok(stream);
                }
                Err(ApiError::Transport(
                    unauthorized_transport @ TransportError::Http { status, .. },
                )) if status == StatusCode::UNAUTHORIZED => {
                    handle_unauthorized(unauthorized_transport, session_telemetry).await?;
                    continue;
                }
                Err(err) => return Err(map_api_error(err)),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Streams a single model request within the current turn.
    ///
    /// The caller is responsible for passing per-turn settings explicitly (model selection,
    /// reasoning settings, and turn metadata). DarwinCode uses HTTP/SSE provider transports in
    /// this runtime path.
    pub async fn stream(
        &mut self,
        provider: &SharedModelProvider,
        prompt: &Prompt,
        model_info: &ModelInfo,
        session_telemetry: &SessionTelemetry,
        effort: Option<ReasoningEffortConfig>,
        summary: ReasoningSummaryConfig,
        service_tier: Option<ServiceTier>,
        turn_metadata_header: Option<&str>,
    ) -> Result<ResponseStream> {
        let wire_api = provider.info().wire_api;
        match wire_api {
            WireApi::Responses => {
                self.stream_responses_api(
                    provider,
                    prompt,
                    model_info,
                    session_telemetry,
                    effort,
                    summary,
                    service_tier,
                    turn_metadata_header,
                )
                .await
            }
            WireApi::ChatCompletions => {
                self.stream_chat_completions_api(
                    provider,
                    prompt,
                    model_info,
                    session_telemetry,
                    effort,
                    turn_metadata_header,
                )
                .await
            }
            WireApi::GeminiGenerate | WireApi::ClaudeMessages => {
                Err(map_api_error(ApiError::Stream(
                    "Native Gemini/Claude streaming not yet implemented in ModelClient".to_string(),
                )))
            }
        }
    }
}

/// Parses per-turn metadata into an HTTP header value.
///
/// Invalid values are treated as absent so callers can compare and propagate
/// metadata with the same sanitization path used when constructing headers.
fn parse_turn_metadata_header(turn_metadata_header: Option<&str>) -> Option<HeaderValue> {
    turn_metadata_header.and_then(|value| HeaderValue::from_str(value).ok())
}

fn chat_messages_from_prompt(prompt: &Prompt) -> Vec<ChatCompletionsMessage> {
    let mut messages = Vec::new();
    let instructions = prompt.base_instructions.text.trim();
    if !instructions.is_empty() {
        messages.push(ChatCompletionsMessage::text("system", instructions));
    }

    let mut current_assistant_msg: Option<ChatCompletionsMessage> = None;
    let mut pending_tool_call_ids: Vec<String> = Vec::new();
    let mut deferred_messages: Vec<ChatCompletionsMessage> = Vec::new();
    let push_current = |messages: &mut Vec<ChatCompletionsMessage>,
                        current: &mut Option<ChatCompletionsMessage>,
                        pending_tool_call_ids: &mut Vec<String>| {
        if let Some(msg) = current.take() {
            pending_tool_call_ids.extend(msg.tool_calls.iter().map(|tc| tc.id.clone()));
            messages.push(msg);
        }
    };
    let flush_deferred =
        |messages: &mut Vec<ChatCompletionsMessage>,
         pending_tool_call_ids: &[String],
         deferred_messages: &mut Vec<ChatCompletionsMessage>| {
            if pending_tool_call_ids.is_empty() {
                messages.append(deferred_messages);
            }
        };

    for item in prompt.get_formatted_input() {
        match item {
            ResponseItem::Message {
                role,
                content,
                reasoning_content,
                ..
            } => {
                let role_str = chat_role(&role);
                let text = content_items_to_plain_text(&content);

                if role_str == "assistant" {
                    let has_open_tool_calls = current_assistant_msg
                        .as_ref()
                        .is_some_and(|msg| !msg.tool_calls.is_empty())
                        || !pending_tool_call_ids.is_empty();
                    push_current(
                        &mut messages,
                        &mut current_assistant_msg,
                        &mut pending_tool_call_ids,
                    );
                    if !text.is_empty() || reasoning_content.is_some() {
                        let mut msg = ChatCompletionsMessage::text(role_str, text);
                        msg.reasoning_content = reasoning_content.clone();
                        if has_open_tool_calls {
                            deferred_messages.push(msg);
                        } else {
                            current_assistant_msg = Some(msg);
                        }
                    }
                } else {
                    if !text.is_empty() {
                        let msg = ChatCompletionsMessage::text(role_str, text);
                        if current_assistant_msg
                            .as_ref()
                            .is_some_and(|msg| !msg.tool_calls.is_empty())
                            || !pending_tool_call_ids.is_empty()
                        {
                            deferred_messages.push(msg);
                        } else {
                            push_current(
                                &mut messages,
                                &mut current_assistant_msg,
                                &mut pending_tool_call_ids,
                            );
                            messages.push(msg);
                        }
                    }
                }
            }
            ResponseItem::FunctionCall {
                name,
                arguments,
                call_id,
                ..
            } => {
                let tc = ChatCompletionsToolCall {
                    id: call_id.clone(),
                    r#type: "function".to_string(),
                    function: ChatCompletionsFunctionCall {
                        name: name.clone(),
                        arguments: arguments.clone(),
                    },
                };
                if let Some(msg) = &mut current_assistant_msg {
                    msg.tool_calls.push(tc);
                } else {
                    current_assistant_msg =
                        Some(ChatCompletionsMessage::assistant_tool_calls(vec![tc]));
                }
            }
            ResponseItem::LocalShellCall {
                id,
                call_id,
                action,
                ..
            } => {
                if let Some(call_id) = call_id.clone().or_else(|| id.clone()) {
                    let tc = ChatCompletionsToolCall {
                        id: call_id,
                        r#type: "function".to_string(),
                        function: ChatCompletionsFunctionCall {
                            name: "local_shell".to_string(),
                            arguments: local_shell_action_to_chat_arguments(&action),
                        },
                    };
                    if let Some(msg) = &mut current_assistant_msg {
                        msg.tool_calls.push(tc);
                    } else {
                        current_assistant_msg =
                            Some(ChatCompletionsMessage::assistant_tool_calls(vec![tc]));
                    }
                }
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                push_current(
                    &mut messages,
                    &mut current_assistant_msg,
                    &mut pending_tool_call_ids,
                );
                messages.push(ChatCompletionsMessage::tool_output(
                    call_id.clone(),
                    function_output_to_plain_text(output),
                ));
                pending_tool_call_ids.retain(|id| id != &call_id);
                flush_deferred(
                    &mut messages,
                    &pending_tool_call_ids,
                    &mut deferred_messages,
                );
            }
            ResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                ..
            } => {
                let tc = ChatCompletionsToolCall {
                    id: call_id.clone(),
                    r#type: "function".to_string(),
                    function: ChatCompletionsFunctionCall {
                        name: name.clone(),
                        arguments: input.clone(),
                    },
                };
                if let Some(msg) = &mut current_assistant_msg {
                    msg.tool_calls.push(tc);
                } else {
                    current_assistant_msg =
                        Some(ChatCompletionsMessage::assistant_tool_calls(vec![tc]));
                }
            }
            ResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => {
                push_current(
                    &mut messages,
                    &mut current_assistant_msg,
                    &mut pending_tool_call_ids,
                );
                messages.push(ChatCompletionsMessage::tool_output(
                    call_id.clone(),
                    function_output_to_plain_text(output),
                ));
                pending_tool_call_ids.retain(|id| id != &call_id);
                flush_deferred(
                    &mut messages,
                    &pending_tool_call_ids,
                    &mut deferred_messages,
                );
            }
            _ => {}
        }
    }

    push_current(
        &mut messages,
        &mut current_assistant_msg,
        &mut pending_tool_call_ids,
    );
    for call_id in pending_tool_call_ids.drain(..) {
        messages.push(ChatCompletionsMessage::tool_output(call_id, "aborted"));
    }
    flush_deferred(
        &mut messages,
        &pending_tool_call_ids,
        &mut deferred_messages,
    );

    messages
}

fn local_shell_action_to_chat_arguments(action: &LocalShellAction) -> String {
    match action {
        LocalShellAction::Exec(exec) => serde_json::json!({
            "command": exec.command,
            "workdir": exec.working_directory,
            "timeout_ms": exec.timeout_ms,
        })
        .to_string(),
    }
}

fn chat_role(role: &str) -> &str {
    match role {
        "system" | "assistant" | "tool" => role,
        _ => "user",
    }
}

fn content_items_to_plain_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                Some(text.as_str())
            }
            ContentItem::InputImage { image_url } => Some(image_url.as_str()),
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn function_output_to_plain_text(
    output: darwin_code_protocol::models::FunctionCallOutputPayload,
) -> String {
    match output.body {
        FunctionCallOutputBody::Text(text) => text,
        FunctionCallOutputBody::ContentItems(items) => items
            .into_iter()
            .map(|item| match item {
                darwin_code_protocol::models::FunctionCallOutputContentItem::InputText { text } => {
                    text
                }
                darwin_code_protocol::models::FunctionCallOutputContentItem::InputImage {
                    image_url,
                    ..
                } => image_url,
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn create_tools_json_for_chat_completions_api(
    tools: &[ToolSpec],
) -> Result<Vec<serde_json::Value>> {
    let mut tools_json = Vec::new();
    for tool in tools {
        match tool {
            ToolSpec::Function(tool) => tools_json.push(chat_function_tool_json(
                &tool.name,
                &tool.description,
                &serde_json::to_value(&tool.parameters)?,
                tool.strict,
            )),
            ToolSpec::Freeform(tool) if tool.name == "apply_patch" => {
                tools_json.push(serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "input": {
                                    "type": "string",
                                    "description": "The full apply_patch payload."
                                }
                            },
                            "required": ["input"],
                            "additionalProperties": false
                        },
                        "strict": false
                    }
                }));
            }
            ToolSpec::ToolSearch { .. }
            | ToolSpec::Namespace(_)
            | ToolSpec::LocalShell {}
            | ToolSpec::ImageGeneration { .. }
            | ToolSpec::WebSearch { .. }
            | ToolSpec::Freeform(_) => {
                trace!(
                    tool = tool.name(),
                    "omitting Responses-only tool for chat completions provider"
                );
            }
        }
    }
    Ok(tools_json)
}

fn chat_function_tool_json(
    name: &str,
    description: &str,
    parameters: &serde_json::Value,
    strict: bool,
) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters,
            "strict": strict
        }
    })
}

/// Builds the extra headers attached to Responses API requests.
///
/// These headers implement DarwinCode-specific conventions:
///
/// - `x-darwin_code-beta-features`: comma-separated beta feature keys enabled for the session.
/// - `x-darwin_code-turn-state`: sticky routing token captured earlier in the turn.
/// - `x-darwin_code-turn-metadata`: optional per-turn metadata for observability.
fn build_responses_headers(
    beta_features_header: Option<&str>,
    turn_state: Option<&Arc<OnceLock<String>>>,
    turn_metadata_header: Option<&HeaderValue>,
) -> ApiHeaderMap {
    let mut headers = ApiHeaderMap::new();
    if let Some(value) = beta_features_header
        && !value.is_empty()
        && let Ok(header_value) = HeaderValue::from_str(value)
    {
        headers.insert("x-darwin_code-beta-features", header_value);
    }
    if let Some(turn_state) = turn_state
        && let Some(state) = turn_state.get()
        && let Ok(header_value) = HeaderValue::from_str(state)
    {
        headers.insert(X_DARWIN_CODE_TURN_STATE_HEADER, header_value);
    }
    if let Some(header_value) = turn_metadata_header {
        headers.insert(X_DARWIN_CODE_TURN_METADATA_HEADER, header_value.clone());
    }
    headers
}

fn subagent_header_value(session_source: &SessionSource) -> Option<String> {
    let SessionSource::SubAgent(subagent_source) = session_source else {
        return None;
    };
    match subagent_source {
        SubAgentSource::Review => Some("review".to_string()),
        SubAgentSource::Compact => Some("compact".to_string()),
        SubAgentSource::MemoryConsolidation => Some("memory_consolidation".to_string()),
        SubAgentSource::ThreadSpawn { .. } => Some("collab_spawn".to_string()),
        SubAgentSource::Other(label) => Some(label.clone()),
    }
}

fn parent_thread_id_header_value(session_source: &SessionSource) -> Option<String> {
    match session_source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        }) => Some(parent_thread_id.to_string()),
        SessionSource::Cli
        | SessionSource::VSCode
        | SessionSource::Exec
        | SessionSource::Mcp
        | SessionSource::Custom(_)
        | SessionSource::SubAgent(_)
        | SessionSource::Unknown => None,
    }
}

fn map_response_stream<S>(api_stream: S, session_telemetry: SessionTelemetry) -> ResponseStream
where
    S: futures::Stream<Item = std::result::Result<ResponseEvent, ApiError>>
        + Unpin
        + Send
        + 'static,
{
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent>>(1600);

    tokio::spawn(async move {
        let mut logged_error = false;
        let mut api_stream = api_stream;
        while let Some(event) = api_stream.next().await {
            match event {
                Ok(event @ ResponseEvent::OutputItemDone(_)) => {
                    if tx_event.send(Ok(event)).await.is_err() {
                        return;
                    }
                }
                Ok(ResponseEvent::Completed {
                    response_id,
                    token_usage,
                }) => {
                    if let Some(usage) = &token_usage {
                        session_telemetry.sse_event_completed(
                            usage.input_tokens,
                            usage.output_tokens,
                            Some(usage.cached_input_tokens),
                            Some(usage.reasoning_output_tokens),
                            usage.total_tokens,
                        );
                    }
                    if tx_event
                        .send(Ok(ResponseEvent::Completed {
                            response_id,
                            token_usage,
                        }))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(event) => {
                    if tx_event.send(Ok(event)).await.is_err() {
                        return;
                    }
                }
                Err(err) => {
                    let mapped = map_api_error(err);
                    if !logged_error {
                        session_telemetry.see_event_completed_failed(&mapped);
                        logged_error = true;
                    }
                    if tx_event.send(Err(mapped)).await.is_err() {
                        return;
                    }
                }
            }
        }
    });

    ResponseStream { rx_event }
}

/// Handles a 401 response without attempting managed account credential renewal.
///
/// Records auth recovery diagnostics and returns the mapped transport error.
async fn handle_unauthorized(
    transport: TransportError,
    session_telemetry: &SessionTelemetry,
) -> Result<()> {
    let debug = extract_response_debug_context(&transport);
    session_telemetry.record_auth_recovery(
        "none",
        "recovery_not_run",
        "skipped",
        debug.request_id.as_deref(),
        debug.cf_ray.as_deref(),
        debug.auth_error.as_deref(),
        debug.auth_error_code.as_deref(),
        Some("byok_auth_has_no_token_refresh"),
        /*auth_state_changed*/ None,
    );
    emit_feedback_auth_recovery_tags(
        "none",
        "recovery_not_run",
        "skipped",
        debug.request_id.as_deref(),
        debug.cf_ray.as_deref(),
        debug.auth_error.as_deref(),
        debug.auth_error_code.as_deref(),
    );
    Err(map_api_error(ApiError::Transport(transport)))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
