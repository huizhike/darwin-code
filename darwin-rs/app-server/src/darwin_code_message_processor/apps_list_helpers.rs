use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use darwin_code_app_server_protocol::AppInfo;
use darwin_code_app_server_protocol::AppListUpdatedNotification;
use darwin_code_app_server_protocol::AppsListResponse;
use darwin_code_app_server_protocol::JSONRPCErrorError;
use darwin_code_app_server_protocol::ServerNotification;

use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::OutgoingMessageSender;

pub(super) fn merge_loaded_apps(
    all_connectors: Option<&[AppInfo]>,
    accessible_connectors: Option<&[AppInfo]>,
) -> Vec<AppInfo> {
    let Some(all_connectors) = all_connectors else {
        return accessible_connectors.map_or_else(Vec::new, <[AppInfo]>::to_vec);
    };
    let Some(accessible_connectors) = accessible_connectors else {
        return all_connectors.to_vec();
    };

    let directory_by_id: HashMap<&str, &AppInfo> = all_connectors
        .iter()
        .map(|connector| (connector.id.as_str(), connector))
        .collect();
    let mut emitted_accessible_ids = HashSet::new();
    let mut merged = Vec::with_capacity(all_connectors.len().max(accessible_connectors.len()));

    for accessible in accessible_connectors {
        let mut connector = match directory_by_id.get(accessible.id.as_str()) {
            Some(directory) => {
                let mut connector = (*directory).clone();
                connector.name = accessible.name.clone();
                connector.is_accessible = true;
                connector.plugin_display_names = accessible.plugin_display_names.clone();
                if connector.description.is_none() {
                    connector.description = accessible.description.clone();
                }
                if connector.logo_url.is_none() {
                    connector.logo_url = accessible.logo_url.clone();
                }
                if connector.logo_url_dark.is_none() {
                    connector.logo_url_dark = accessible.logo_url_dark.clone();
                }
                connector
            }
            None => accessible.clone(),
        };
        connector.is_accessible = true;
        emitted_accessible_ids.insert(accessible.id.as_str());
        merged.push(connector);
    }

    for connector in all_connectors {
        if !emitted_accessible_ids.contains(connector.id.as_str()) {
            merged.push(connector.clone());
        }
    }

    merged
}

pub(super) fn should_send_app_list_updated_notification(
    connectors: &[AppInfo],
    accessible_loaded: bool,
    all_loaded: bool,
) -> bool {
    connectors.iter().any(|connector| connector.is_accessible) || (accessible_loaded && all_loaded)
}

pub(super) fn paginate_apps(
    connectors: &[AppInfo],
    start: usize,
    limit: Option<u32>,
) -> Result<AppsListResponse, JSONRPCErrorError> {
    let total = connectors.len();
    if start > total {
        return Err(JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("cursor {start} exceeds total apps {total}"),
            data: None,
        });
    }

    let effective_limit = limit.unwrap_or(total as u32).max(1) as usize;
    let end = start.saturating_add(effective_limit).min(total);
    let data = connectors[start..end].to_vec();
    let next_cursor = if end < total {
        Some(end.to_string())
    } else {
        None
    };

    Ok(AppsListResponse { data, next_cursor })
}

pub(super) async fn send_app_list_updated_notification(
    outgoing: &Arc<OutgoingMessageSender>,
    data: Vec<AppInfo>,
) {
    outgoing
        .send_server_notification(ServerNotification::AppListUpdated(
            AppListUpdatedNotification { data },
        ))
        .await;
}
