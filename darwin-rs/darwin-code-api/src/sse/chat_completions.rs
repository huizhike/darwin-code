use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::error::ApiError;
use crate::rate_limits::parse_all_rate_limits;
use darwin_code_client::ByteStream;
use darwin_code_client::StreamResponse;
use darwin_code_protocol::models::ContentItem;
use darwin_code_protocol::models::ResponseItem;
use darwin_code_protocol::protocol::TokenUsage;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;

pub fn spawn_chat_completions_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
) -> ResponseStream {
    let rate_limit_snapshots = parse_all_rate_limits(&stream_response.headers);
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(async move {
        for snapshot in rate_limit_snapshots {
            let _ = tx_event.send(Ok(ResponseEvent::RateLimits(snapshot))).await;
        }
        process_sse(stream_response.bytes, tx_event, idle_timeout).await;
    });

    ResponseStream { rx_event }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsChunk {
    id: Option<String>,
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChatCompletionsChoice>,
    usage: Option<ChatCompletionsUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsChoice {
    delta: ChatCompletionsDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatCompletionsDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatCompletionsToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<ChatCompletionsFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsUsage {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
    completion_tokens_details: Option<ChatCompletionsCompletionTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsCompletionTokenDetails {
    reasoning_tokens: Option<i64>,
}

impl From<ChatCompletionsUsage> for TokenUsage {
    fn from(value: ChatCompletionsUsage) -> Self {
        let input_tokens = value.prompt_tokens.unwrap_or(0);
        let output_tokens = value.completion_tokens.unwrap_or(0);
        let reasoning_output_tokens = value
            .completion_tokens_details
            .and_then(|details| details.reasoning_tokens)
            .unwrap_or(0);
        TokenUsage {
            input_tokens,
            cached_input_tokens: 0,
            output_tokens,
            reasoning_output_tokens,
            total_tokens: value
                .total_tokens
                .unwrap_or(input_tokens + output_tokens + reasoning_output_tokens),
        }
    }
}

#[derive(Debug, Default)]
struct ToolCallAccumulator {
    id: Option<String>,
    name: String,
    arguments: String,
}

#[derive(Debug)]
struct ChatStreamState {
    response_id: String,
    message_id: String,
    message_started: bool,
    text: String,
    reasoning_text: String,
    tool_calls: BTreeMap<usize, ToolCallAccumulator>,
    token_usage: Option<TokenUsage>,
    server_model: Option<String>,
}

impl Default for ChatStreamState {
    fn default() -> Self {
        Self {
            response_id: "chatcmpl-local".to_string(),
            message_id: "chatcmpl-message-0".to_string(),
            message_started: false,
            text: String::new(),
            reasoning_text: String::new(),
            tool_calls: BTreeMap::new(),
            token_usage: None,
            server_model: None,
        }
    }
}

impl ChatStreamState {
    async fn ensure_message_started(
        &mut self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> bool {
        if self.message_started {
            return true;
        }
        self.message_started = true;
        let item = ResponseItem::Message {
            id: Some(self.message_id.clone()),
            role: "assistant".to_string(),
            content: Vec::new(),
            end_turn: None,
            phase: None,
            reasoning_content: None,
        };
        tx_event
            .send(Ok(ResponseEvent::OutputItemAdded(item)))
            .await
            .is_ok()
    }

    fn absorb_chunk(&mut self, chunk: ChatCompletionsChunk) {
        if let Some(id) = chunk.id {
            self.response_id = id;
        }
        if let Some(model) = chunk.model {
            self.server_model = Some(model);
        }
        if let Some(usage) = chunk.usage {
            self.token_usage = Some(usage.into());
        }
        for choice in chunk.choices {
            let _finish_reason = choice.finish_reason;
            for tool_call in choice.delta.tool_calls {
                let acc = self.tool_calls.entry(tool_call.index).or_default();
                if let Some(id) = tool_call.id {
                    acc.id = Some(id);
                }
                if let Some(function) = tool_call.function {
                    if let Some(name) = function.name {
                        acc.name.push_str(&name);
                    }
                    if let Some(arguments) = function.arguments {
                        acc.arguments.push_str(&arguments);
                    }
                }
            }
        }
    }

    async fn finish(
        self,
        tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    ) -> Result<(), ()> {
        if self.message_started {
            let item = ResponseItem::Message {
                id: Some(self.message_id),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText { text: self.text }],
                end_turn: None,
                phase: None,
                reasoning_content: (!self.reasoning_text.is_empty()).then_some(self.reasoning_text),
            };
            tx_event
                .send(Ok(ResponseEvent::OutputItemDone(item)))
                .await
                .map_err(|_| ())?;
        }

        for (_index, tool_call) in self.tool_calls {
            if tool_call.name.is_empty() {
                continue;
            }
            let call_id = tool_call
                .id
                .unwrap_or_else(|| format!("call_{}", tool_call.name));
            let item = ResponseItem::FunctionCall {
                id: None,
                name: tool_call.name,
                namespace: None,
                arguments: tool_call.arguments,
                call_id,
            };
            tx_event
                .send(Ok(ResponseEvent::OutputItemDone(item)))
                .await
                .map_err(|_| ())?;
        }

        tx_event
            .send(Ok(ResponseEvent::Completed {
                response_id: self.response_id,
                token_usage: self.token_usage,
            }))
            .await
            .map_err(|_| ())
    }
}

pub async fn process_sse(
    stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
) {
    let mut stream = stream.eventsource();
    let mut state = ChatStreamState::default();

    loop {
        let response = timeout(idle_timeout, stream.next()).await;
        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                debug!("Chat completions SSE error: {e:#}");
                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                return;
            }
            Ok(None) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream(
                        "stream closed before chat completion finished".into(),
                    )))
                    .await;
                return;
            }
            Err(_) => {
                let _ = tx_event
                    .send(Err(ApiError::Stream("idle timeout waiting for SSE".into())))
                    .await;
                return;
            }
        };

        let data = sse.data.trim();
        trace!("Chat completions SSE event: {data}");
        if data == "[DONE]" {
            let _ = state.finish(&tx_event).await;
            return;
        }

        let chunk: ChatCompletionsChunk = match serde_json::from_str(data) {
            Ok(chunk) => chunk,
            Err(e) => {
                debug!("Failed to parse chat completions SSE chunk: {e}, data: {data}");
                continue;
            }
        };

        if let Some(model) = chunk.model.as_ref()
            && state.server_model.as_deref() != Some(model.as_str())
            && tx_event
                .send(Ok(ResponseEvent::ServerModel(model.clone())))
                .await
                .is_err()
        {
            return;
        }

        for choice in &chunk.choices {
            if let Some(reasoning) = choice.delta.reasoning_content.as_deref()
                && !reasoning.is_empty()
            {
                if !state.ensure_message_started(&tx_event).await {
                    return;
                }
                state.reasoning_text.push_str(reasoning);
                if tx_event
                    .send(Ok(ResponseEvent::ReasoningContentDelta {
                        delta: reasoning.to_string(),
                        content_index: 0,
                    }))
                    .await
                    .is_err()
                {
                    return;
                }
            }

            if let Some(content) = choice.delta.content.as_deref()
                && !content.is_empty()
            {
                if !state.ensure_message_started(&tx_event).await {
                    return;
                }
                state.text.push_str(content);
                if tx_event
                    .send(Ok(ResponseEvent::OutputTextDelta(content.to_string())))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        state.absorb_chunk(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use darwin_code_client::TransportError;
    use futures::stream;
    use pretty_assertions::assert_eq;

    async fn collect_events(chunks: Vec<&'static str>) -> Vec<Result<ResponseEvent, ApiError>> {
        let byte_chunks = chunks
            .into_iter()
            .map(|chunk| Ok(Bytes::from(chunk)))
            .collect::<Vec<Result<Bytes, TransportError>>>();
        let stream = Box::pin(stream::iter(byte_chunks));
        let (tx_event, mut rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);
        tokio::spawn(process_sse(stream, tx_event, Duration::from_secs(5)));
        let mut events = Vec::new();
        while let Some(event) = rx_event.recv().await {
            events.push(event);
            if matches!(events.last(), Some(Ok(ResponseEvent::Completed { .. }))) {
                break;
            }
        }
        events
    }

    #[tokio::test]
    async fn chat_completions_text_delta_reaches_response_events() {
        let events = collect_events(vec![
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"deepseek-v4-flash\",\"choices\":[{\"delta\":{\"content\":\"he\"},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"delta\":{\"content\":\"llo\"},\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(matches!(
            events.first(),
            Some(Ok(ResponseEvent::ServerModel(model))) if model == "deepseek-v4-flash"
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            Ok(ResponseEvent::OutputTextDelta(delta)) if delta == "he"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            Ok(ResponseEvent::OutputTextDelta(delta)) if delta == "llo"
        )));
        let done = events
            .iter()
            .find_map(|event| match event {
                Ok(ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. })) => {
                    Some(content)
                }
                _ => None,
            })
            .expect("assistant message should finish");
        assert_eq!(
            done,
            &vec![ContentItem::OutputText {
                text: "hello".to_string()
            }]
        );
        assert!(matches!(
            events.last(),
            Some(Ok(ResponseEvent::Completed { response_id, .. })) if response_id == "chatcmpl-1"
        ));
    }

    #[tokio::test]
    async fn chat_completions_reasoning_content_is_non_fatal() {
        let events = collect_events(vec![
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"delta\":{\"reasoning_content\":\"thinking\"},\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        assert!(matches!(
            events.last(),
            Some(Ok(ResponseEvent::Completed { .. }))
        ));
    }

    #[tokio::test]
    async fn chat_completions_tool_call_chunks_are_aggregated() {
        let events = collect_events(vec![
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"exec_command\",\"arguments\":\"{\\\"cmd\\\":\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"ls\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        ])
        .await;

        let call = events
            .iter()
            .find_map(|event| match event {
                Ok(ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
                    name,
                    arguments,
                    call_id,
                    ..
                })) => Some((name, arguments, call_id)),
                _ => None,
            })
            .expect("function call should be emitted");
        assert_eq!(call.0, "exec_command");
        assert_eq!(call.1, "{\"cmd\":\"ls\"}");
        assert_eq!(call.2, "call_1");
    }
}
