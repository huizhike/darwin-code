use super::AuthRequestTelemetryContext;
use super::ModelClient;
use super::PendingUnauthorizedRetry;
use super::UnauthorizedRecoveryExecution;
use super::X_DARWIN_CODE_INSTALLATION_ID_HEADER;
use super::X_DARWIN_CODE_PARENT_THREAD_ID_HEADER;
use super::X_DARWIN_CODE_TURN_METADATA_HEADER;
use super::X_DARWIN_CODE_WINDOW_ID_HEADER;
use super::X_OPENAI_SUBAGENT_HEADER;
use super::chat_messages_from_prompt;
use super::create_tools_json_for_chat_completions_api;
use darwin_code_model_provider::BearerAuthProvider;
use darwin_code_model_provider_info::ModelProviderInfo;
use darwin_code_otel::SessionTelemetry;
use darwin_code_protocol::ThreadId;
use darwin_code_protocol::models::BaseInstructions;
use darwin_code_protocol::models::ContentItem;
use darwin_code_protocol::models::FunctionCallOutputPayload;
use darwin_code_protocol::models::ResponseItem;
use darwin_code_protocol::openai_models::ModelInfo;
use darwin_code_protocol::protocol::SessionSource;
use darwin_code_protocol::protocol::SubAgentSource;
use darwin_code_tools::JsonSchema;
use darwin_code_tools::ResponsesApiTool;
use darwin_code_tools::ToolSpec;
use pretty_assertions::assert_eq;
use serde_json::json;

fn test_model_client(session_source: SessionSource) -> ModelClient {
    let provider =
        ModelProviderInfo::create_openai_provider(Some("https://example.com/v1".to_string()), None);
    ModelClient::new(
        ThreadId::new(),
        /*installation_id*/ "11111111-1111-4111-8111-111111111111".to_string(),
        provider,
        session_source,
        /*model_verbosity*/ None,
        /*enable_request_compression*/ false,
        /*include_timing_metrics*/ false,
        /*beta_features_header*/ None,
    )
}

fn test_model_info() -> ModelInfo {
    serde_json::from_value(json!({
        "slug": "gpt-test",
        "display_name": "gpt-test",
        "description": "desc",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "medium", "description": "medium"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "upgrade": null,
        "base_instructions": "base instructions",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": 10000},
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": 272000,
        "auto_compact_token_limit": null,
        "experimental_supported_tools": []
    }))
    .expect("deserialize test model info")
}

fn test_session_telemetry() -> SessionTelemetry {
    SessionTelemetry::new(
        ThreadId::new(),
        "gpt-test",
        "gpt-test",
        /*account_id*/ None,
        /*account_email*/ None,
        /*auth_mode*/ None,
        "test-originator".to_string(),
        /*log_user_prompts*/ false,
        "test-terminal".to_string(),
        SessionSource::Cli,
    )
}

#[test]
fn chat_completions_messages_include_system_user_and_tool_output() {
    let prompt = crate::client_common::Prompt {
        input: vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hi".to_string(),
                }],
        end_turn: None,
        phase: None,
        reasoning_content: None,
    },
            ResponseItem::FunctionCall {
                id: None,
                name: "exec_command".to_string(),
                namespace: None,
                arguments: "{\"cmd\":\"ls\"}".to_string(),
                call_id: "call_1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: "be helpful".to_string(),
        },
        personality: None,
        output_schema: None,
    };

    let messages = chat_messages_from_prompt(&prompt);
    assert_eq!(messages[0].role, "system");
    assert_eq!(messages[0].content.as_deref(), Some("be helpful"));
    assert_eq!(messages[1].role, "user");
    assert_eq!(messages[1].content.as_deref(), Some("hi"));
    assert_eq!(messages[2].role, "assistant");
    assert_eq!(messages[2].tool_calls[0].function.name, "exec_command");
    assert_eq!(messages[3].role, "tool");
    assert_eq!(messages[3].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(messages[3].content.as_deref(), Some("ok"));
}

#[test]
fn chat_completions_tools_use_function_shape() {
    let tools =
        create_tools_json_for_chat_completions_api(&[ToolSpec::Function(ResponsesApiTool {
            name: "exec_command".to_string(),
            description: "run command".to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                std::collections::BTreeMap::new(),
                Some(Vec::new()),
                Some(false.into()),
            ),
            output_schema: None,
        })])
        .expect("function tools should serialize");

    assert_eq!(
        tools,
        vec![json!({
            "type": "function",
            "function": {
                "name": "exec_command",
                "description": "run command",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": [],
                    "additionalProperties": false
                },
                "strict": false
            }
        })]
    );
}

#[test]
fn build_subagent_headers_sets_other_subagent_label() {
    let client = test_model_client(SessionSource::SubAgent(SubAgentSource::Other(
        "memory_consolidation".to_string(),
    )));
    let headers = client.build_subagent_headers();
    let value = headers
        .get(X_OPENAI_SUBAGENT_HEADER)
        .and_then(|value| value.to_str().ok());
    assert_eq!(value, Some("memory_consolidation"));
}

#[test]
fn build_ws_client_metadata_includes_window_lineage_and_turn_metadata() {
    let parent_thread_id = ThreadId::new();
    let client = test_model_client(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id,
        depth: 2,
        agent_path: None,
        agent_nickname: None,
        agent_role: None,
    }));

    client.advance_window_generation();

    let client_metadata = client.build_ws_client_metadata(Some(r#"{"turn_id":"turn-123"}"#));
    let conversation_id = client.state.conversation_id;
    assert_eq!(
        client_metadata,
        std::collections::HashMap::from([
            (
                X_DARWIN_CODE_INSTALLATION_ID_HEADER.to_string(),
                "11111111-1111-4111-8111-111111111111".to_string(),
            ),
            (
                X_DARWIN_CODE_WINDOW_ID_HEADER.to_string(),
                format!("{conversation_id}:1"),
            ),
            (
                X_OPENAI_SUBAGENT_HEADER.to_string(),
                "collab_spawn".to_string(),
            ),
            (
                X_DARWIN_CODE_PARENT_THREAD_ID_HEADER.to_string(),
                parent_thread_id.to_string(),
            ),
            (
                X_DARWIN_CODE_TURN_METADATA_HEADER.to_string(),
                r#"{"turn_id":"turn-123"}"#.to_string(),
            ),
        ])
    );
}

#[tokio::test]
async fn summarize_memories_returns_empty_for_empty_input() {
    let client = test_model_client(SessionSource::Cli);
    let model_info = test_model_info();
    let session_telemetry = test_session_telemetry();

    let output = client
        .summarize_memories(
            Vec::new(),
            &model_info,
            /*effort*/ None,
            &session_telemetry,
        )
        .await
        .expect("empty summarize request should succeed");
    assert_eq!(output.len(), 0);
}

#[test]
fn auth_request_telemetry_context_tracks_attached_auth_and_retry_phase() {
    let auth_context = AuthRequestTelemetryContext::new(
        &BearerAuthProvider::for_test(Some("access-token")),
        PendingUnauthorizedRetry::from_recovery(UnauthorizedRecoveryExecution {
            mode: "byok",
            phase: "unauthorized",
        }),
    );

    assert_eq!(auth_context.auth_mode, None);
    assert!(auth_context.auth_header_attached);
    assert_eq!(auth_context.auth_header_name, Some("authorization"));
    assert!(auth_context.retry_after_unauthorized);
    assert_eq!(auth_context.recovery_mode, Some("byok"));
    assert_eq!(auth_context.recovery_phase, Some("unauthorized"));
}
