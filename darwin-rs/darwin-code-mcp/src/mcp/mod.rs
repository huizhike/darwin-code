pub(crate) mod auth;
mod skill_dependencies;
pub use auth::McpAuthStatusEntry;
pub use auth::compute_auth_statuses;
pub use skill_dependencies::canonical_mcp_server_key;
pub use skill_dependencies::collect_missing_mcp_dependencies;

use std::collections::HashMap;
use std::path::PathBuf;

use async_channel::unbounded;
use darwin_code_config::Constrained;
use darwin_code_config::McpServerConfig;
use darwin_code_plugin::PluginCapabilitySummary;
use darwin_code_protocol::mcp::Resource;
use darwin_code_protocol::mcp::ResourceTemplate;
use darwin_code_protocol::mcp::Tool;
use darwin_code_protocol::protocol::AskForApproval;
use darwin_code_protocol::protocol::McpAuthStatus;
use darwin_code_protocol::protocol::McpListToolsResponseEvent;
use darwin_code_protocol::protocol::SandboxPolicy;
use serde_json::Value;

use crate::mcp_connection_manager::McpConnectionManager;
use crate::mcp_connection_manager::darwin_code_apps_tools_cache_key;
pub type McpManager = McpConnectionManager;

const MCP_TOOL_NAME_PREFIX: &str = "mcp";
const MCP_TOOL_NAME_DELIMITER: &str = "__";
pub const DARWIN_CODE_APPS_MCP_SERVER_NAME: &str = "darwin_code_apps";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum McpSnapshotDetail {
    #[default]
    Full,
    ToolsAndAuthOnly,
}

impl McpSnapshotDetail {
    fn include_resources(self) -> bool {
        matches!(self, Self::Full)
    }
}

/// The Responses API requires tool names to match `^[a-zA-Z0-9_-]+$`.
/// MCP server/tool names are user-controlled, so sanitize the fully-qualified
/// name we expose to the model by replacing any disallowed character with `_`.
pub(crate) fn sanitize_responses_api_tool_name(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            sanitized.push(c);
        } else {
            sanitized.push('_');
        }
    }

    if sanitized.is_empty() {
        "_".to_string()
    } else {
        sanitized
    }
}

pub fn qualified_mcp_tool_name_prefix(server_name: &str) -> String {
    sanitize_responses_api_tool_name(&format!(
        "{MCP_TOOL_NAME_PREFIX}{MCP_TOOL_NAME_DELIMITER}{server_name}{MCP_TOOL_NAME_DELIMITER}"
    ))
}

/// Returns true when MCP permission prompts should resolve as approved instead
/// of being shown to the user.
pub fn mcp_permission_prompt_is_auto_approved(
    approval_policy: AskForApproval,
    sandbox_policy: &SandboxPolicy,
) -> bool {
    approval_policy == AskForApproval::Never
        && matches!(
            sandbox_policy,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        )
}

/// MCP runtime settings derived from `darwin_code_core::config::Config`.
///
/// This struct should contain only long-lived configuration values that the
/// `darwin-code-mcp` crate needs to construct server transports, enforce MCP
/// approval/sandbox policy, and merge plugin-provided MCP servers.
/// Request-scoped auth state should not be stored here;
/// thread those values explicitly into runtime entry points such as
/// [`with_darwin_code_apps_mcp`] and [`collect_mcp_snapshot`] so config objects do
/// not go stale when auth changes.
#[derive(Debug, Clone)]
pub struct McpConfig {
    /// DarwinCode home directory used for app-tool cache files.
    pub darwin_code_home: PathBuf,
    /// Whether skill MCP dependency installation prompts are enabled.
    pub skill_mcp_dependency_install_enabled: bool,
    /// Approval policy used for MCP tool calls and MCP elicitation requests.
    pub approval_policy: Constrained<AskForApproval>,
    /// Optional path to `darwin-code-linux-sandbox` for sandboxed MCP tool execution.
    pub darwin_code_linux_sandbox_exe: Option<PathBuf>,
    /// Whether to use legacy Landlock behavior in the MCP sandbox state.
    pub use_legacy_landlock: bool,
    /// Whether the app MCP integration is enabled by config.
    pub apps_enabled: bool,
    /// User-configured and plugin-provided MCP servers keyed by server name.
    pub configured_mcp_servers: HashMap<String, McpServerConfig>,
    /// Plugin metadata used to attribute MCP tools/connectors to plugin display names.
    pub plugin_capability_summaries: Vec<PluginCapabilitySummary>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolPluginProvenance {
    plugin_display_names_by_connector_id: HashMap<String, Vec<String>>,
    plugin_display_names_by_mcp_server_name: HashMap<String, Vec<String>>,
}

impl ToolPluginProvenance {
    pub fn plugin_display_names_for_connector_id(&self, connector_id: &str) -> &[String] {
        self.plugin_display_names_by_connector_id
            .get(connector_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn plugin_display_names_for_mcp_server_name(&self, server_name: &str) -> &[String] {
        self.plugin_display_names_by_mcp_server_name
            .get(server_name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn from_capability_summaries(capability_summaries: &[PluginCapabilitySummary]) -> Self {
        let mut tool_plugin_provenance = Self::default();
        for plugin in capability_summaries {
            for connector_id in &plugin.app_connector_ids {
                tool_plugin_provenance
                    .plugin_display_names_by_connector_id
                    .entry(connector_id.0.clone())
                    .or_default()
                    .push(plugin.display_name.clone());
            }

            for server_name in &plugin.mcp_server_names {
                tool_plugin_provenance
                    .plugin_display_names_by_mcp_server_name
                    .entry(server_name.clone())
                    .or_default()
                    .push(plugin.display_name.clone());
            }
        }

        for plugin_names in tool_plugin_provenance
            .plugin_display_names_by_connector_id
            .values_mut()
            .chain(
                tool_plugin_provenance
                    .plugin_display_names_by_mcp_server_name
                    .values_mut(),
            )
        {
            plugin_names.sort_unstable();
            plugin_names.dedup();
        }

        tool_plugin_provenance
    }
}

pub fn with_darwin_code_apps_mcp(
    mut servers: HashMap<String, McpServerConfig>,
    _config: &McpConfig,
) -> HashMap<String, McpServerConfig> {
    // Built-in built-in apps MCP is intentionally disabled in the BYOK-only local engine.
    servers.remove(DARWIN_CODE_APPS_MCP_SERVER_NAME);
    servers
}

pub fn configured_mcp_servers(config: &McpConfig) -> HashMap<String, McpServerConfig> {
    config.configured_mcp_servers.clone()
}

pub fn effective_mcp_servers(config: &McpConfig) -> HashMap<String, McpServerConfig> {
    let servers = configured_mcp_servers(config);
    with_darwin_code_apps_mcp(servers, config)
}

pub fn tool_plugin_provenance(config: &McpConfig) -> ToolPluginProvenance {
    ToolPluginProvenance::from_capability_summaries(&config.plugin_capability_summaries)
}

pub async fn collect_mcp_snapshot(
    config: &McpConfig,
    submit_id: String,
) -> McpListToolsResponseEvent {
    collect_mcp_snapshot_with_detail(config, submit_id, McpSnapshotDetail::Full).await
}

pub async fn collect_mcp_snapshot_with_detail(
    config: &McpConfig,
    submit_id: String,
    detail: McpSnapshotDetail,
) -> McpListToolsResponseEvent {
    let mcp_servers = effective_mcp_servers(config);
    let tool_plugin_provenance = tool_plugin_provenance(config);
    if mcp_servers.is_empty() {
        return McpListToolsResponseEvent {
            tools: HashMap::new(),
            resources: HashMap::new(),
            resource_templates: HashMap::new(),
            auth_statuses: HashMap::new(),
        };
    }

    let auth_status_entries = compute_auth_statuses(mcp_servers.iter()).await;

    let (tx_event, rx_event) = unbounded();
    drop(rx_event);

    let (mcp_connection_manager, cancel_token) = McpConnectionManager::new(
        &mcp_servers,
        auth_status_entries.clone(),
        &config.approval_policy,
        submit_id,
        tx_event,
        SandboxPolicy::new_read_only_policy(),
        config.darwin_code_home.clone(),
        darwin_code_apps_tools_cache_key(),
        tool_plugin_provenance,
    )
    .await;

    let snapshot = collect_mcp_snapshot_from_manager_with_detail(
        &mcp_connection_manager,
        auth_status_entries,
        detail,
    )
    .await;

    cancel_token.cancel();

    snapshot
}

#[derive(Debug, Clone)]
pub struct McpServerStatusSnapshot {
    pub tools_by_server: HashMap<String, HashMap<String, Tool>>,
    pub resources: HashMap<String, Vec<Resource>>,
    pub resource_templates: HashMap<String, Vec<ResourceTemplate>>,
    pub auth_statuses: HashMap<String, McpAuthStatus>,
}

pub async fn collect_mcp_server_status_snapshot(
    config: &McpConfig,
    submit_id: String,
) -> McpServerStatusSnapshot {
    collect_mcp_server_status_snapshot_with_detail(config, submit_id, McpSnapshotDetail::Full).await
}

pub async fn collect_mcp_server_status_snapshot_with_detail(
    config: &McpConfig,
    submit_id: String,
    detail: McpSnapshotDetail,
) -> McpServerStatusSnapshot {
    let mcp_servers = effective_mcp_servers(config);
    let tool_plugin_provenance = tool_plugin_provenance(config);
    if mcp_servers.is_empty() {
        return McpServerStatusSnapshot {
            tools_by_server: HashMap::new(),
            resources: HashMap::new(),
            resource_templates: HashMap::new(),
            auth_statuses: HashMap::new(),
        };
    }

    let auth_status_entries = compute_auth_statuses(mcp_servers.iter()).await;

    let (tx_event, rx_event) = unbounded();
    drop(rx_event);

    let (mcp_connection_manager, cancel_token) = McpConnectionManager::new(
        &mcp_servers,
        auth_status_entries.clone(),
        &config.approval_policy,
        submit_id,
        tx_event,
        SandboxPolicy::new_read_only_policy(),
        config.darwin_code_home.clone(),
        darwin_code_apps_tools_cache_key(),
        tool_plugin_provenance,
    )
    .await;

    let snapshot = collect_mcp_server_status_snapshot_from_manager(
        &mcp_connection_manager,
        auth_status_entries,
        detail,
    )
    .await;

    cancel_token.cancel();

    snapshot
}

pub fn split_qualified_tool_name(qualified_name: &str) -> Option<(String, String)> {
    let mut parts = qualified_name.split(MCP_TOOL_NAME_DELIMITER);
    let prefix = parts.next()?;
    if prefix != MCP_TOOL_NAME_PREFIX {
        return None;
    }
    let server_name = parts.next()?;
    let tool_name: String = parts.collect::<Vec<_>>().join(MCP_TOOL_NAME_DELIMITER);
    if tool_name.is_empty() {
        return None;
    }
    Some((server_name.to_string(), tool_name))
}

pub fn group_tools_by_server(
    tools: &HashMap<String, Tool>,
) -> HashMap<String, HashMap<String, Tool>> {
    let mut grouped = HashMap::new();
    for (qualified_name, tool) in tools {
        if let Some((server_name, tool_name)) = split_qualified_tool_name(qualified_name) {
            grouped
                .entry(server_name)
                .or_insert_with(HashMap::new)
                .insert(tool_name, tool.clone());
        }
    }
    grouped
}

fn protocol_tool_from_rmcp_tool(name: &str, tool: &rmcp::model::Tool) -> Option<Tool> {
    match serde_json::to_value(tool) {
        Ok(value) => match Tool::from_mcp_value(value) {
            Ok(tool) => Some(tool),
            Err(err) => {
                tracing::warn!("Failed to convert MCP tool '{name}': {err}");
                None
            }
        },
        Err(err) => {
            tracing::warn!("Failed to serialize MCP tool '{name}': {err}");
            None
        }
    }
}

fn auth_statuses_from_entries(
    auth_status_entries: &HashMap<String, crate::mcp::auth::McpAuthStatusEntry>,
) -> HashMap<String, McpAuthStatus> {
    auth_status_entries
        .iter()
        .map(|(name, entry)| (name.clone(), entry.auth_status))
        .collect::<HashMap<_, _>>()
}

fn convert_mcp_resources(
    resources: HashMap<String, Vec<rmcp::model::Resource>>,
) -> HashMap<String, Vec<Resource>> {
    resources
        .into_iter()
        .map(|(name, resources)| {
            let resources = resources
                .into_iter()
                .filter_map(|resource| match serde_json::to_value(resource) {
                    Ok(value) => match Resource::from_mcp_value(value.clone()) {
                        Ok(resource) => Some(resource),
                        Err(err) => {
                            let (uri, resource_name) = match value {
                                Value::Object(obj) => (
                                    obj.get("uri")
                                        .and_then(|v| v.as_str().map(ToString::to_string)),
                                    obj.get("name")
                                        .and_then(|v| v.as_str().map(ToString::to_string)),
                                ),
                                _ => (None, None),
                            };

                            tracing::warn!(
                                "Failed to convert MCP resource (uri={uri:?}, name={resource_name:?}): {err}"
                            );
                            None
                        }
                    },
                    Err(err) => {
                        tracing::warn!("Failed to serialize MCP resource: {err}");
                        None
                    }
                })
                .collect::<Vec<_>>();
            (name, resources)
        })
        .collect::<HashMap<_, _>>()
}

fn convert_mcp_resource_templates(
    resource_templates: HashMap<String, Vec<rmcp::model::ResourceTemplate>>,
) -> HashMap<String, Vec<ResourceTemplate>> {
    resource_templates
        .into_iter()
        .map(|(name, templates)| {
            let templates = templates
                .into_iter()
                .filter_map(|template| match serde_json::to_value(template) {
                    Ok(value) => match ResourceTemplate::from_mcp_value(value.clone()) {
                        Ok(template) => Some(template),
                        Err(err) => {
                            let (uri_template, template_name) = match value {
                                Value::Object(obj) => (
                                    obj.get("uriTemplate")
                                        .or_else(|| obj.get("uri_template"))
                                        .and_then(|v| v.as_str().map(ToString::to_string)),
                                    obj.get("name")
                                        .and_then(|v| v.as_str().map(ToString::to_string)),
                                ),
                                _ => (None, None),
                            };

                            tracing::warn!(
                                "Failed to convert MCP resource template (uri_template={uri_template:?}, name={template_name:?}): {err}"
                            );
                            None
                        }
                    },
                    Err(err) => {
                        tracing::warn!("Failed to serialize MCP resource template: {err}");
                        None
                    }
                })
                .collect::<Vec<_>>();
            (name, templates)
        })
        .collect::<HashMap<_, _>>()
}

async fn collect_mcp_server_status_snapshot_from_manager(
    mcp_connection_manager: &McpConnectionManager,
    auth_status_entries: HashMap<String, crate::mcp::auth::McpAuthStatusEntry>,
    detail: McpSnapshotDetail,
) -> McpServerStatusSnapshot {
    let (tools, resources, resource_templates) = tokio::join!(
        mcp_connection_manager.list_all_tools(),
        async {
            if detail.include_resources() {
                mcp_connection_manager.list_all_resources().await
            } else {
                HashMap::new()
            }
        },
        async {
            if detail.include_resources() {
                mcp_connection_manager.list_all_resource_templates().await
            } else {
                HashMap::new()
            }
        },
    );

    let mut tools_by_server = HashMap::<String, HashMap<String, Tool>>::new();
    for (_qualified_name, tool_info) in tools {
        let raw_tool_name = tool_info.tool.name.to_string();
        let Some(tool) = protocol_tool_from_rmcp_tool(&raw_tool_name, &tool_info.tool) else {
            continue;
        };
        let tool_name = tool.name.clone();
        tools_by_server
            .entry(tool_info.server_name)
            .or_default()
            .insert(tool_name, tool);
    }

    McpServerStatusSnapshot {
        tools_by_server,
        resources: convert_mcp_resources(resources),
        resource_templates: convert_mcp_resource_templates(resource_templates),
        auth_statuses: auth_statuses_from_entries(&auth_status_entries),
    }
}

pub async fn collect_mcp_snapshot_from_manager(
    mcp_connection_manager: &McpConnectionManager,
    auth_status_entries: HashMap<String, McpAuthStatusEntry>,
) -> McpListToolsResponseEvent {
    collect_mcp_snapshot_from_manager_with_detail(
        mcp_connection_manager,
        auth_status_entries,
        McpSnapshotDetail::Full,
    )
    .await
}

pub async fn collect_mcp_snapshot_from_manager_with_detail(
    mcp_connection_manager: &McpConnectionManager,
    auth_status_entries: HashMap<String, McpAuthStatusEntry>,
    detail: McpSnapshotDetail,
) -> McpListToolsResponseEvent {
    let (tools, resources, resource_templates) = tokio::join!(
        mcp_connection_manager.list_all_tools(),
        async {
            if detail.include_resources() {
                mcp_connection_manager.list_all_resources().await
            } else {
                HashMap::new()
            }
        },
        async {
            if detail.include_resources() {
                mcp_connection_manager.list_all_resource_templates().await
            } else {
                HashMap::new()
            }
        },
    );

    let tools = tools
        .into_iter()
        .filter_map(|(name, tool)| {
            protocol_tool_from_rmcp_tool(&name, &tool.tool).map(|tool| (name, tool))
        })
        .collect::<HashMap<_, _>>();

    McpListToolsResponseEvent {
        tools,
        resources: convert_mcp_resources(resources),
        resource_templates: convert_mcp_resource_templates(resource_templates),
        auth_statuses: auth_statuses_from_entries(&auth_status_entries),
    }
}
