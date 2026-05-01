use crate::session::session::Session;
use darwin_code_protocol::error::DarwinCodeErr;
use darwin_code_protocol::error::Result as DarwinCodeResult;
use darwin_code_protocol::protocol::ConversationAudioParams;
use darwin_code_protocol::protocol::ConversationStartParams;
use darwin_code_protocol::protocol::ConversationTextParams;
use darwin_code_protocol::protocol::DarwinCodeErrorInfo;
use darwin_code_protocol::protocol::ErrorEvent;
use darwin_code_protocol::protocol::Event;
use darwin_code_protocol::protocol::EventMsg;
use darwin_code_protocol::protocol::RealtimeAudioFrame;
use darwin_code_protocol::protocol::RealtimeConversationClosedEvent;
use darwin_code_protocol::protocol::RealtimeConversationRealtimeEvent;
use darwin_code_protocol::protocol::RealtimeEvent;
use std::sync::Arc;

pub(crate) const REALTIME_USER_TEXT_PREFIX: &str = "[USER] ";
#[allow(dead_code)]
pub(crate) const REALTIME_BACKEND_TEXT_PREFIX: &str = "[BACKEND] ";

const REALTIME_DISABLED_MESSAGE: &str =
    "realtime model transport has been removed; DarwinCode is HTTP/SSE-only";

pub(crate) struct RealtimeConversationManager;

impl RealtimeConversationManager {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn running_state(&self) -> Option<()> {
        None
    }

    pub(crate) async fn is_running_v2(&self) -> bool {
        false
    }

    #[allow(dead_code)]
    pub(crate) async fn audio_in(&self, _frame: RealtimeAudioFrame) -> DarwinCodeResult<()> {
        Err(not_running())
    }

    pub(crate) async fn text_in(&self, _text: String) -> DarwinCodeResult<()> {
        Err(not_running())
    }

    pub(crate) async fn handoff_out(&self, _output_text: String) -> DarwinCodeResult<()> {
        Ok(())
    }

    pub(crate) async fn handoff_complete(&self) -> DarwinCodeResult<()> {
        Ok(())
    }

    pub(crate) async fn active_handoff_id(&self) -> Option<String> {
        None
    }

    pub(crate) async fn clear_active_handoff(&self) {}

    pub(crate) async fn shutdown(&self) -> DarwinCodeResult<()> {
        Ok(())
    }
}

pub(crate) async fn handle_start(
    sess: &Arc<Session>,
    sub_id: String,
    _params: ConversationStartParams,
) -> DarwinCodeResult<()> {
    send_realtime_disabled(sess, sub_id.clone()).await;
    send_realtime_conversation_closed(sess, sub_id, Some("unsupported".to_string())).await;
    Ok(())
}

pub(crate) async fn handle_audio(
    sess: &Arc<Session>,
    sub_id: String,
    _params: ConversationAudioParams,
) {
    send_conversation_error(
        sess,
        sub_id,
        REALTIME_DISABLED_MESSAGE.to_string(),
        DarwinCodeErrorInfo::BadRequest,
    )
    .await;
}

pub(crate) async fn handle_text(
    sess: &Arc<Session>,
    sub_id: String,
    _params: ConversationTextParams,
) {
    send_conversation_error(
        sess,
        sub_id,
        REALTIME_DISABLED_MESSAGE.to_string(),
        DarwinCodeErrorInfo::BadRequest,
    )
    .await;
}

pub(crate) async fn handle_close(sess: &Arc<Session>, sub_id: String) {
    let _ = sess.conversation.shutdown().await;
    send_realtime_conversation_closed(sess, sub_id, Some("requested".to_string())).await;
}

pub(crate) fn prefix_realtime_v2_text(text: String, prefix: &str) -> String {
    if text.is_empty() || text.starts_with(prefix) {
        text
    } else {
        format!("{prefix}{text}")
    }
}

fn not_running() -> DarwinCodeErr {
    DarwinCodeErr::InvalidRequest("conversation is not running".to_string())
}

async fn send_realtime_disabled(sess: &Arc<Session>, sub_id: String) {
    sess.send_event_raw(Event {
        id: sub_id,
        msg: EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::Error(REALTIME_DISABLED_MESSAGE.to_string()),
        }),
    })
    .await;
}

async fn send_realtime_conversation_closed(
    sess: &Arc<Session>,
    sub_id: String,
    reason: Option<String>,
) {
    sess.send_event_raw(Event {
        id: sub_id,
        msg: EventMsg::RealtimeConversationClosed(RealtimeConversationClosedEvent { reason }),
    })
    .await;
}

async fn send_conversation_error(
    sess: &Arc<Session>,
    sub_id: String,
    message: String,
    darwin_code_error_info: DarwinCodeErrorInfo,
) {
    sess.send_event_raw(Event {
        id: sub_id,
        msg: EventMsg::Error(ErrorEvent {
            message,
            darwin_code_error_info: Some(darwin_code_error_info),
        }),
    })
    .await;
}
