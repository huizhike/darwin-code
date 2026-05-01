use crate::session::session::Session;
use crate::tools::sandboxing::ToolError;
use darwin_code_sandboxing::NetworkAccessRuntime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkApprovalMode {
    Immediate,
    Deferred,
}

/// Historical tool-runtime hook for network policy approval.
///
/// DarwinCode no longer starts an in-process network access, so this spec is
/// accepted for compatibility with tool runtimes but ignored at runtime.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct NetworkApprovalSpec {
    pub network: Option<NetworkAccessRuntime>,
    pub mode: NetworkApprovalMode,
    pub command: String,
}

#[derive(Clone, Debug)]
pub(crate) struct DeferredNetworkApproval {
    registration_id: String,
}

impl DeferredNetworkApproval {
    pub(crate) fn registration_id(&self) -> &str {
        &self.registration_id
    }
}

#[derive(Debug)]
pub(crate) struct ActiveNetworkApproval {
    registration_id: Option<String>,
    mode: NetworkApprovalMode,
}

impl ActiveNetworkApproval {
    pub(crate) fn mode(&self) -> NetworkApprovalMode {
        self.mode
    }

    pub(crate) fn into_deferred(self) -> Option<DeferredNetworkApproval> {
        match (self.mode, self.registration_id) {
            (NetworkApprovalMode::Deferred, Some(registration_id)) => {
                Some(DeferredNetworkApproval { registration_id })
            }
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct NetworkApprovalService;

impl NetworkApprovalService {
    pub(crate) async fn sync_session_approved_hosts_to(&self, _other: &Self) {}

    pub(crate) async fn unregister_call(&self, _registration_id: &str) {}
}

pub(crate) async fn begin_network_approval(
    _session: &Session,
    _turn_id: &str,
    _network_policy_active: bool,
    _spec: Option<NetworkApprovalSpec>,
) -> Option<ActiveNetworkApproval> {
    None
}

pub(crate) async fn finish_immediate_network_approval(
    _session: &Session,
    _network_approval: ActiveNetworkApproval,
) -> Result<(), ToolError> {
    Ok(())
}

pub(crate) async fn finish_deferred_network_approval(
    _session: &Session,
    _deferred: Option<DeferredNetworkApproval>,
) {
}
