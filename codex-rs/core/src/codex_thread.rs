use crate::agent::AgentStatus;
use crate::config::ConstraintResult;
use crate::file_watcher::WatchRegistration;
use crate::session::DarwinCode;
use crate::session::SteerInputError;
use darwin_code_features::Feature;
use darwin_code_protocol::config_types::ApprovalsReviewer;
use darwin_code_protocol::config_types::Personality;
use darwin_code_protocol::config_types::ServiceTier;
use darwin_code_protocol::error::DarwinCodeErr;
use darwin_code_protocol::error::Result as DarwinCodeResult;
use darwin_code_protocol::mcp::CallToolResult;
use darwin_code_protocol::models::ContentItem;
use darwin_code_protocol::models::ResponseInputItem;
use darwin_code_protocol::models::ResponseItem;
use darwin_code_protocol::openai_models::ReasoningEffort;
use darwin_code_protocol::protocol::AskForApproval;
use darwin_code_protocol::protocol::Event;
use darwin_code_protocol::protocol::Op;
use darwin_code_protocol::protocol::SandboxPolicy;
use darwin_code_protocol::protocol::SessionSource;
use darwin_code_protocol::protocol::Submission;
use darwin_code_protocol::protocol::ThreadMemoryMode;
use darwin_code_protocol::protocol::TokenUsage;
use darwin_code_protocol::protocol::TokenUsageInfo;
use darwin_code_protocol::protocol::W3cTraceContext;
use darwin_code_protocol::user_input::UserInput;
use darwin_code_utils_absolute_path::AbsolutePathBuf;
use rmcp::model::ReadResourceRequestParams;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tokio::sync::watch;

use darwin_code_rollout::state_db::StateDbHandle;

#[derive(Clone, Debug)]
pub struct ThreadConfigSnapshot {
    pub model: String,
    pub model_provider_id: String,
    pub service_tier: Option<ServiceTier>,
    pub approval_policy: AskForApproval,
    pub approvals_reviewer: ApprovalsReviewer,
    pub sandbox_policy: SandboxPolicy,
    pub cwd: AbsolutePathBuf,
    pub ephemeral: bool,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub personality: Option<Personality>,
    pub session_source: SessionSource,
}

pub struct DarwinCodeThread {
    pub(crate) darwin_code: DarwinCode,
    rollout_path: Option<PathBuf>,
    out_of_band_elicitation_count: Mutex<u64>,
    _watch_registration: WatchRegistration,
}

/// Conduit for the bidirectional stream of messages that compose a thread
/// (formerly called a conversation) in DarwinCode.
impl DarwinCodeThread {
    pub(crate) fn new(
        darwin_code: DarwinCode,
        rollout_path: Option<PathBuf>,
        watch_registration: WatchRegistration,
    ) -> Self {
        Self {
            darwin_code,
            rollout_path,
            out_of_band_elicitation_count: Mutex::new(0),
            _watch_registration: watch_registration,
        }
    }

    pub async fn submit(&self, op: Op) -> DarwinCodeResult<String> {
        self.darwin_code.submit(op).await
    }

    pub async fn shutdown_and_wait(&self) -> DarwinCodeResult<()> {
        self.darwin_code.shutdown_and_wait().await
    }

    #[doc(hidden)]
    pub async fn ensure_rollout_materialized(&self) {
        self.darwin_code.session.ensure_rollout_materialized().await;
    }

    #[doc(hidden)]
    pub async fn flush_rollout(&self) -> std::io::Result<()> {
        self.darwin_code.session.flush_rollout().await
    }

    pub async fn submit_with_trace(
        &self,
        op: Op,
        trace: Option<W3cTraceContext>,
    ) -> DarwinCodeResult<String> {
        self.darwin_code.submit_with_trace(op, trace).await
    }

    /// Persist whether this thread is eligible for future memory generation.
    pub async fn set_thread_memory_mode(&self, mode: ThreadMemoryMode) -> anyhow::Result<()> {
        self.darwin_code.set_thread_memory_mode(mode).await
    }

    pub async fn steer_input(
        &self,
        input: Vec<UserInput>,
        expected_turn_id: Option<&str>,
        responsesapi_client_metadata: Option<HashMap<String, String>>,
    ) -> Result<String, SteerInputError> {
        self.darwin_code
            .steer_input(input, expected_turn_id, responsesapi_client_metadata)
            .await
    }

    pub async fn set_app_server_client_info(
        &self,
        app_server_client_name: Option<String>,
        app_server_client_version: Option<String>,
    ) -> ConstraintResult<()> {
        self.darwin_code
            .set_app_server_client_info(app_server_client_name, app_server_client_version)
            .await
    }

    /// Use sparingly: this is intended to be removed soon.
    pub async fn submit_with_id(&self, sub: Submission) -> DarwinCodeResult<()> {
        self.darwin_code.submit_with_id(sub).await
    }

    pub async fn next_event(&self) -> DarwinCodeResult<Event> {
        self.darwin_code.next_event().await
    }

    pub async fn agent_status(&self) -> AgentStatus {
        self.darwin_code.agent_status().await
    }

    pub(crate) fn subscribe_status(&self) -> watch::Receiver<AgentStatus> {
        self.darwin_code.agent_status.clone()
    }

    pub(crate) async fn total_token_usage(&self) -> Option<TokenUsage> {
        self.darwin_code.session.total_token_usage().await
    }

    /// Returns the complete token usage snapshot currently cached for this thread.
    ///
    /// This accessor is intentionally narrower than direct session access: it lets
    /// app-server lifecycle paths replay restored usage after resume or fork without
    /// exposing broader session mutation authority. A caller that only reads
    /// `total_token_usage` would drop last-turn usage and make the v2
    /// `thread/tokenUsage/updated` payload incomplete.
    pub async fn token_usage_info(&self) -> Option<TokenUsageInfo> {
        self.darwin_code.session.token_usage_info().await
    }

    /// Records a user-role session-prefix message without creating a new user turn boundary.
    pub(crate) async fn inject_user_message_without_turn(&self, message: String) {
        let message = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: message }],
            end_turn: None,
            phase: None,
            reasoning_content: None,
        };
        let pending_item = match pending_message_input_item(&message) {
            Ok(pending_item) => pending_item,
            Err(err) => {
                debug_assert!(false, "session-prefix message append should succeed: {err}");
                return;
            }
        };
        if self
            .darwin_code
            .session
            .inject_response_items(vec![pending_item])
            .await
            .is_err()
        {
            let turn_context = self.darwin_code.session.new_default_turn().await;
            self.darwin_code
                .session
                .record_conversation_items(turn_context.as_ref(), &[message])
                .await;
        }
    }

    /// Append a prebuilt message to the thread history without treating it as a user turn.
    ///
    /// If the thread already has an active turn, the message is queued as pending input for that
    /// turn. Otherwise it is queued at session scope and a regular turn is started so the agent
    /// can consume that pending input through the normal turn pipeline.
    #[cfg(test)]
    pub(crate) async fn append_message(&self, message: ResponseItem) -> DarwinCodeResult<String> {
        let submission_id = uuid::Uuid::new_v4().to_string();
        let pending_item = pending_message_input_item(&message)?;
        if let Err(items) = self
            .darwin_code
            .session
            .inject_response_items(vec![pending_item])
            .await
        {
            self.darwin_code
                .session
                .queue_response_items_for_next_turn(items)
                .await;
            self.darwin_code
                .session
                .maybe_start_turn_for_pending_work()
                .await;
        }

        Ok(submission_id)
    }

    /// Append raw Responses API items to the thread's model-visible history.
    pub async fn inject_response_items(&self, items: Vec<ResponseItem>) -> DarwinCodeResult<()> {
        if items.is_empty() {
            return Err(DarwinCodeErr::InvalidRequest(
                "items must not be empty".to_string(),
            ));
        }

        let turn_context = self.darwin_code.session.new_default_turn().await;
        if self
            .darwin_code
            .session
            .reference_context_item()
            .await
            .is_none()
        {
            self.darwin_code
                .session
                .record_context_updates_and_set_reference_context_item(turn_context.as_ref())
                .await;
        }
        self.darwin_code
            .session
            .record_conversation_items(turn_context.as_ref(), &items)
            .await;
        self.darwin_code.session.flush_rollout().await?;
        Ok(())
    }

    pub fn rollout_path(&self) -> Option<PathBuf> {
        self.rollout_path.clone()
    }

    pub fn state_db(&self) -> Option<StateDbHandle> {
        self.darwin_code.state_db()
    }

    pub async fn config_snapshot(&self) -> ThreadConfigSnapshot {
        self.darwin_code.thread_config_snapshot().await
    }

    pub async fn read_mcp_resource(
        &self,
        server: &str,
        uri: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let result = self
            .darwin_code
            .session
            .read_resource(
                server,
                ReadResourceRequestParams {
                    meta: None,
                    uri: uri.to_string(),
                },
            )
            .await?;

        Ok(serde_json::to_value(result)?)
    }

    pub async fn call_mcp_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<serde_json::Value>,
        meta: Option<serde_json::Value>,
    ) -> anyhow::Result<CallToolResult> {
        self.darwin_code
            .session
            .call_tool(server, tool, arguments, meta)
            .await
    }

    pub fn enabled(&self, feature: Feature) -> bool {
        self.darwin_code.enabled(feature)
    }

    pub async fn increment_out_of_band_elicitation_count(&self) -> DarwinCodeResult<u64> {
        let mut guard = self.out_of_band_elicitation_count.lock().await;
        let was_zero = *guard == 0;
        *guard = guard.checked_add(1).ok_or_else(|| {
            DarwinCodeErr::Fatal("out-of-band elicitation count overflowed".to_string())
        })?;

        if was_zero {
            self.darwin_code
                .session
                .set_out_of_band_elicitation_pause_state(/*paused*/ true);
        }

        Ok(*guard)
    }

    pub async fn decrement_out_of_band_elicitation_count(&self) -> DarwinCodeResult<u64> {
        let mut guard = self.out_of_band_elicitation_count.lock().await;
        if *guard == 0 {
            return Err(DarwinCodeErr::InvalidRequest(
                "out-of-band elicitation count is already zero".to_string(),
            ));
        }

        *guard -= 1;
        let now_zero = *guard == 0;
        if now_zero {
            self.darwin_code
                .session
                .set_out_of_band_elicitation_pause_state(/*paused*/ false);
        }

        Ok(*guard)
    }
}

fn pending_message_input_item(message: &ResponseItem) -> DarwinCodeResult<ResponseInputItem> {
    match message {
        ResponseItem::Message { role, content, .. } => Ok(ResponseInputItem::Message {
            role: role.clone(),
            content: content.clone(),
        }),
        _ => Err(DarwinCodeErr::InvalidRequest(
            "append_message only supports ResponseItem::Message".to_string(),
        )),
    }
}
