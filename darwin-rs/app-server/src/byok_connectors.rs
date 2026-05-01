use std::collections::HashSet;

use darwin_code_app_server_protocol::AppInfo;
use darwin_code_core::config::Config;

pub(crate) async fn list_cached_accessible_connectors_from_mcp_tools(
    _config: &Config,
) -> Option<Vec<AppInfo>> {
    None
}

pub(crate) async fn list_cached_all_connectors(_config: &Config) -> Option<Vec<AppInfo>> {
    None
}

pub(crate) async fn list_accessible_connectors_from_mcp_tools_with_options(
    _config: &Config,
    _force_refetch: bool,
) -> anyhow::Result<Vec<AppInfo>> {
    Ok(Vec::new())
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
