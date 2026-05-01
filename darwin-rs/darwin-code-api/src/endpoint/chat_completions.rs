use crate::auth::SharedAuthProvider;
use crate::common::ChatCompletionsApiRequest;
use crate::common::ResponseStream;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::requests::headers::build_conversation_headers;
use crate::requests::headers::insert_header;
use crate::requests::headers::subagent_header;
use crate::sse::spawn_chat_completions_stream;
use darwin_code_client::HttpTransport;
use darwin_code_protocol::protocol::SessionSource;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use tracing::instrument;

pub struct ChatCompletionsClient<T: HttpTransport> {
    session: EndpointSession<T>,
}

#[derive(Default)]
pub struct ChatCompletionsOptions {
    pub conversation_id: Option<String>,
    pub session_source: Option<SessionSource>,
    pub extra_headers: HeaderMap,
}

impl<T: HttpTransport> ChatCompletionsClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
        }
    }

    #[instrument(
        name = "chat_completions.stream_request",
        level = "info",
        skip_all,
        fields(
            transport = "chat_completions_http",
            http.method = "POST",
            api.path = "chat/completions"
        )
    )]
    pub async fn stream_request(
        &self,
        request: ChatCompletionsApiRequest,
        options: ChatCompletionsOptions,
    ) -> Result<ResponseStream, ApiError> {
        let ChatCompletionsOptions {
            conversation_id,
            session_source,
            extra_headers,
        } = options;

        let body = serde_json::to_value(&request).map_err(|e| {
            ApiError::Stream(format!("failed to encode chat completions request: {e}"))
        })?;

        let mut headers = extra_headers;
        if let Some(ref conv_id) = conversation_id {
            insert_header(&mut headers, "x-client-request-id", conv_id);
        }
        headers.extend(build_conversation_headers(conversation_id));
        if let Some(subagent) = subagent_header(&session_source) {
            insert_header(&mut headers, "x-darwin-subagent", &subagent);
        }

        self.stream(body, headers).await
    }

    fn path() -> &'static str {
        "chat/completions"
    }

    #[instrument(
        name = "chat_completions.stream",
        level = "info",
        skip_all,
        fields(
            transport = "chat_completions_http",
            http.method = "POST",
            api.path = "chat/completions"
        )
    )]
    pub async fn stream(
        &self,
        body: serde_json::Value,
        extra_headers: HeaderMap,
    ) -> Result<ResponseStream, ApiError> {
        let stream_response = self
            .session
            .stream_with(
                Method::POST,
                Self::path(),
                extra_headers,
                Some(body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                },
            )
            .await?;

        Ok(spawn_chat_completions_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_completions_path_is_chat_completions() {
        assert_eq!(
            ChatCompletionsClient::<darwin_code_client::ReqwestTransport>::path(),
            "chat/completions"
        );
    }
}
