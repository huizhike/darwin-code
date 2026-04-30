use std::collections::HashSet;

use crate::legacy_core::config::Config;
use darwin_code_app_server_protocol::AppInfo;

#[derive(Debug, Clone, Default)]
pub(crate) struct AccessibleConnectorsStatus {
    pub(crate) connectors: Vec<AppInfo>,
    pub(crate) codex_apps_ready: bool,
}

pub(crate) async fn list_accessible_connectors_from_mcp_tools_with_options_and_status(
    _config: &Config,
    _force_refetch: bool,
) -> anyhow::Result<AccessibleConnectorsStatus> {
    Ok(AccessibleConnectorsStatus::default())
}

pub(crate) async fn list_all_connectors_with_options(
    _config: &Config,
    _force_refetch: bool,
) -> anyhow::Result<Vec<AppInfo>> {
    Ok(Vec::new())
}

pub(crate) fn merge_connectors_with_accessible(
    all_connectors: Vec<AppInfo>,
    accessible_connectors: Vec<AppInfo>,
    _all_connectors_loaded: bool,
) -> Vec<AppInfo> {
    let accessible_ids = accessible_connectors
        .iter()
        .map(|connector| connector.id.as_str())
        .collect::<HashSet<_>>();
    all_connectors
        .into_iter()
        .map(|mut connector| {
            connector.is_accessible = accessible_ids.contains(connector.id.as_str());
            connector
        })
        .collect()
}

pub(crate) fn with_app_enabled_state(connectors: Vec<AppInfo>, _config: &Config) -> Vec<AppInfo> {
    connectors
}
