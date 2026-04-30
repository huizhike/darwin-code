use darwin_code_state::StateRuntime;
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::TransportEvent;

#[derive(Clone)]
pub(crate) struct RemoteControlHandle {
    enabled_tx: Arc<watch::Sender<bool>>,
}

impl RemoteControlHandle {
    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled_tx.send_if_modified(|state| {
            let changed = *state != enabled;
            *state = enabled;
            changed
        });
    }
}

pub(crate) async fn start_remote_control(
    _state_db: Option<Arc<StateRuntime>>,
    _transport_event_tx: mpsc::Sender<TransportEvent>,
    shutdown_token: CancellationToken,
    _app_server_client_name_rx: Option<oneshot::Receiver<String>>,
    initial_enabled: bool,
) -> io::Result<(JoinHandle<()>, RemoteControlHandle)> {
    let (enabled_tx, _enabled_rx) = watch::channel(initial_enabled);
    let join_handle = tokio::spawn(async move {
        shutdown_token.cancelled().await;
    });
    Ok((
        join_handle,
        RemoteControlHandle {
            enabled_tx: Arc::new(enabled_tx),
        },
    ))
}
