use super::ModelClient;
use super::X_DARWIN_SUBAGENT_HEADER;
use super::chat_messages_from_prompt;
use super::create_tools_json_for_chat_completions_api;
use darwin_code_model_provider_info::ModelProviderInfo;
use darwin_code_protocol::ThreadId;
use darwin_code_protocol::models::BaseInstructions;
use darwin_code_protocol::models::ContentItem;
use darwin_code_protocol::models::FunctionCallOutputPayload;
use darwin_code_protocol::models::LocalShellAction;
use darwin_code_protocol::models::LocalShellExecAction;
use darwin_code_protocol::models::LocalShellStatus;
use darwin_code_protocol::models::ResponseItem;
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
fn chat_completions_messages_keep_tool_outputs_adjacent_to_tool_calls() {
    let prompt = crate::client_common::Prompt {
        input: vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "inspect the bug".to_string(),
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
            ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "Approved command prefix saved:\n- [\"ls\"]".to_string(),
                }],
                end_turn: None,
                phase: None,
                reasoning_content: None,
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: String::new(),
        },
        personality: None,
        output_schema: None,
    };

    let messages = chat_messages_from_prompt(&prompt);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content.as_deref(), Some("inspect the bug"));
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].tool_calls[0].id, "call_1");
    assert_eq!(messages[2].role, "tool");
    assert_eq!(messages[2].tool_call_id.as_deref(), Some("call_1"));
    assert_eq!(messages[2].content.as_deref(), Some("ok"));
    assert_eq!(messages[3].role, "user");
    assert_eq!(
        messages[3].content.as_deref(),
        Some("Approved command prefix saved:\n- [\"ls\"]")
    );
}

#[test]
fn chat_completions_messages_serialize_local_shell_pair() {
    let prompt = crate::client_common::Prompt {
        input: vec![
            ResponseItem::LocalShellCall {
                id: None,
                call_id: Some("call_shell".to_string()),
                status: LocalShellStatus::Completed,
                action: LocalShellAction::Exec(LocalShellExecAction {
                    command: vec!["ls".to_string()],
                    timeout_ms: Some(1000),
                    working_directory: Some("/tmp".to_string()),
                    env: None,
                    user: None,
                }),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_shell".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
        ],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: String::new(),
        },
        personality: None,
        output_schema: None,
    };

    let messages = chat_messages_from_prompt(&prompt);
    assert_eq!(messages[0].role, "assistant");
    assert_eq!(messages[0].tool_calls[0].id, "call_shell");
    assert_eq!(messages[0].tool_calls[0].function.name, "local_shell");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&messages[0].tool_calls[0].function.arguments)
            .expect("local shell arguments should be json"),
        json!({
            "command": ["ls"],
            "workdir": "/tmp",
            "timeout_ms": 1000
        })
    );
    assert_eq!(messages[1].role, "tool");
    assert_eq!(messages[1].tool_call_id.as_deref(), Some("call_shell"));
    assert_eq!(messages[1].content.as_deref(), Some("ok"));
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
        .get(X_DARWIN_SUBAGENT_HEADER)
        .and_then(|value| value.to_str().ok());
    assert_eq!(value, Some("memory_consolidation"));
}
