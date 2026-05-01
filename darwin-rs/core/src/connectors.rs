use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::LazyLock;
use std::sync::Mutex as StdMutex;
use std::time::Instant;

pub use darwin_code_app_server_protocol::AppBranding;
pub use darwin_code_app_server_protocol::AppInfo;
pub use darwin_code_app_server_protocol::AppMetadata;
use darwin_code_tools::DiscoverableTool;
use rmcp::model::ToolAnnotations;
use serde::Deserialize;

use crate::config::Config;
use crate::config_loader::AppsRequirementsToml;
use crate::plugins::PluginsManager;
use darwin_code_client::originator;
use darwin_code_config::types::AppToolApproval;
use darwin_code_config::types::AppsConfigToml;
use darwin_code_config::types::ToolSuggestDiscoverableType;
use darwin_code_mcp::DARWIN_CODE_APPS_MCP_SERVER_NAME;
use darwin_code_mcp::ToolInfo;
use darwin_code_mcp::ToolPluginProvenance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppToolPolicy {
    pub enabled: bool,
    pub approval: AppToolApproval,
}

impl Default for AppToolPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            approval: AppToolApproval::Auto,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct AccessibleConnectorsCacheKey;

#[derive(Clone)]
struct CachedAccessibleConnectors {
    key: AccessibleConnectorsCacheKey,
    expires_at: Instant,
    connectors: Vec<AppInfo>,
}

static ACCESSIBLE_CONNECTORS_CACHE: LazyLock<StdMutex<Option<CachedAccessibleConnectors>>> =
    LazyLock::new(|| StdMutex::new(None));

#[derive(Debug, Clone)]
pub struct AccessibleConnectorsStatus {
    pub connectors: Vec<AppInfo>,
    pub darwin_code_apps_ready: bool,
}

pub async fn list_accessible_connectors_from_mcp_tools(
    config: &Config,
) -> anyhow::Result<Vec<AppInfo>> {
    Ok(
        list_accessible_connectors_from_mcp_tools_with_options_and_status(
            config, /*force_refetch*/ false,
        )
        .await?
        .connectors,
    )
}

pub(crate) async fn list_tool_suggest_discoverable_tools_with_auth(
    config: &Config,
    _auth: Option<()>,
    accessible_connectors: &[AppInfo],
) -> anyhow::Result<Vec<DiscoverableTool>> {
    let directory_connectors = list_directory_connectors_for_tool_suggest(config).await?;
    let connector_ids = tool_suggest_connector_ids(config).await;
    let discoverable_connectors =
        darwin_code_connectors::filter::filter_tool_suggest_discoverable_connectors(
            directory_connectors,
            accessible_connectors,
            &connector_ids,
            originator().value.as_str(),
        )
        .into_iter()
        .map(DiscoverableTool::from);
    Ok(discoverable_connectors.collect())
}

pub async fn list_cached_accessible_connectors_from_mcp_tools(
    config: &Config,
) -> Option<Vec<AppInfo>> {
    if !config.features.apps_enabled() {
        return Some(Vec::new());
    }
    let cache_key = accessible_connectors_cache_key(config, None);
    read_cached_accessible_connectors(&cache_key).map(|connectors| {
        darwin_code_connectors::filter::filter_disallowed_connectors(
            connectors,
            originator().value.as_str(),
        )
    })
}

pub(crate) fn refresh_accessible_connectors_cache_from_mcp_tools(
    config: &Config,
    _auth: Option<()>,
    mcp_tools: &HashMap<String, ToolInfo>,
) {
    if !config.features.apps_enabled() {
        return;
    }

    let cache_key = accessible_connectors_cache_key(config, None);
    let accessible_connectors = darwin_code_connectors::filter::filter_disallowed_connectors(
        accessible_connectors_from_mcp_tools(mcp_tools),
        originator().value.as_str(),
    );
    write_cached_accessible_connectors(cache_key, &accessible_connectors);
}

pub async fn list_accessible_connectors_from_mcp_tools_with_options(
    config: &Config,
    force_refetch: bool,
) -> anyhow::Result<Vec<AppInfo>> {
    Ok(
        list_accessible_connectors_from_mcp_tools_with_options_and_status(config, force_refetch)
            .await?
            .connectors,
    )
}

pub async fn list_accessible_connectors_from_mcp_tools_with_options_and_status(
    config: &Config,
    _force_refetch: bool,
) -> anyhow::Result<AccessibleConnectorsStatus> {
    if !config.features.apps_enabled() {
        return Ok(AccessibleConnectorsStatus {
            connectors: Vec::new(),
            darwin_code_apps_ready: true,
        });
    }

    let cache_key = accessible_connectors_cache_key(config, None);
    let connectors = read_cached_accessible_connectors(&cache_key)
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    let connectors = darwin_code_connectors::filter::filter_disallowed_connectors(
        connectors,
        originator().value.as_str(),
    );
    Ok(AccessibleConnectorsStatus {
        connectors,
        darwin_code_apps_ready: true,
    })
}

fn accessible_connectors_cache_key(
    _config: &Config,
    _auth: Option<()>,
) -> AccessibleConnectorsCacheKey {
    AccessibleConnectorsCacheKey
}

fn read_cached_accessible_connectors(
    cache_key: &AccessibleConnectorsCacheKey,
) -> Option<Vec<AppInfo>> {
    let mut cache_guard = ACCESSIBLE_CONNECTORS_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let now = Instant::now();

    if let Some(cached) = cache_guard.as_ref() {
        if now < cached.expires_at && cached.key == *cache_key {
            return Some(cached.connectors.clone());
        }
        if now >= cached.expires_at {
            *cache_guard = None;
        }
    }

    None
}

fn write_cached_accessible_connectors(
    cache_key: AccessibleConnectorsCacheKey,
    connectors: &[AppInfo],
) {
    let mut cache_guard = ACCESSIBLE_CONNECTORS_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *cache_guard = Some(CachedAccessibleConnectors {
        key: cache_key,
        expires_at: Instant::now() + darwin_code_connectors::CONNECTORS_CACHE_TTL,
        connectors: connectors.to_vec(),
    });
}

async fn tool_suggest_connector_ids(config: &Config) -> HashSet<String> {
    let mut connector_ids = PluginsManager::new(config.darwin_code_home.to_path_buf())
        .plugins_for_config(config)
        .await
        .capability_summaries()
        .iter()
        .flat_map(|plugin| plugin.app_connector_ids.iter())
        .map(|connector_id| connector_id.0.clone())
        .collect::<HashSet<_>>();
    connector_ids.extend(
        config
            .tool_suggest
            .discoverables
            .iter()
            .filter(|discoverable| discoverable.kind == ToolSuggestDiscoverableType::Connector)
            .map(|discoverable| discoverable.id.clone()),
    );
    connector_ids
}

async fn list_directory_connectors_for_tool_suggest(
    _config: &Config,
) -> anyhow::Result<Vec<AppInfo>> {
    // DarwinCode is BYOK-only: account directory connectors are externalized
    // to a gateway/plugin layer and are not fetched from core.
    Ok(Vec::new())
}

pub(crate) fn accessible_connectors_from_mcp_tools(
    mcp_tools: &HashMap<String, ToolInfo>,
) -> Vec<AppInfo> {
    // ToolInfo already carries plugin provenance, so app-level plugin sources
    // can be derived here instead of requiring a separate enrichment pass.
    let tools = mcp_tools.values().filter_map(|tool| {
        if tool.server_name != DARWIN_CODE_APPS_MCP_SERVER_NAME {
            return None;
        }
        let connector_id = tool.connector_id.as_deref()?;
        Some(
            darwin_code_connectors::accessible::AccessibleConnectorTool {
                connector_id: connector_id.to_string(),
                connector_name: tool.connector_name.clone(),
                connector_description: tool.connector_description.clone(),
                plugin_display_names: tool.plugin_display_names.clone(),
            },
        )
    });
    darwin_code_connectors::accessible::collect_accessible_connectors(tools)
}

pub fn with_app_enabled_state(mut connectors: Vec<AppInfo>, config: &Config) -> Vec<AppInfo> {
    let user_apps_config = read_user_apps_config(config);
    let requirements_apps_config = config.config_layer_stack.requirements_toml().apps.as_ref();
    if user_apps_config.is_none() && requirements_apps_config.is_none() {
        return connectors;
    }

    for connector in &mut connectors {
        if let Some(apps_config) = user_apps_config.as_ref()
            && (apps_config.default.is_some()
                || apps_config.apps.contains_key(connector.id.as_str()))
        {
            connector.is_enabled = app_is_enabled(apps_config, Some(connector.id.as_str()));
        }

        if requirements_apps_config
            .and_then(|apps| apps.apps.get(connector.id.as_str()))
            .is_some_and(|app| app.enabled == Some(false))
        {
            connector.is_enabled = false;
        }
    }

    connectors
}

pub fn with_app_plugin_sources(
    mut connectors: Vec<AppInfo>,
    tool_plugin_provenance: &ToolPluginProvenance,
) -> Vec<AppInfo> {
    for connector in &mut connectors {
        connector.plugin_display_names = tool_plugin_provenance
            .plugin_display_names_for_connector_id(connector.id.as_str())
            .to_vec();
    }
    connectors
}

pub(crate) fn app_tool_policy(
    config: &Config,
    connector_id: Option<&str>,
    tool_name: &str,
    tool_title: Option<&str>,
    annotations: Option<&ToolAnnotations>,
) -> AppToolPolicy {
    let apps_config = read_apps_config(config);
    app_tool_policy_from_apps_config(
        apps_config.as_ref(),
        connector_id,
        tool_name,
        tool_title,
        annotations,
    )
}

pub(crate) fn darwin_code_app_tool_is_enabled(config: &Config, tool_info: &ToolInfo) -> bool {
    if tool_info.server_name != DARWIN_CODE_APPS_MCP_SERVER_NAME {
        return true;
    }

    app_tool_policy(
        config,
        tool_info.connector_id.as_deref(),
        &tool_info.tool.name,
        tool_info.tool.title.as_deref(),
        tool_info.tool.annotations.as_ref(),
    )
    .enabled
}

fn read_apps_config(config: &Config) -> Option<AppsConfigToml> {
    let apps_config = read_user_apps_config(config);
    let had_apps_config = apps_config.is_some();
    let mut apps_config = apps_config.unwrap_or_default();
    apply_requirements_apps_constraints(
        &mut apps_config,
        config.config_layer_stack.requirements_toml().apps.as_ref(),
    );
    if had_apps_config || apps_config.default.is_some() || !apps_config.apps.is_empty() {
        Some(apps_config)
    } else {
        None
    }
}

fn read_user_apps_config(config: &Config) -> Option<AppsConfigToml> {
    config
        .config_layer_stack
        .effective_config()
        .as_table()
        .and_then(|table| table.get("apps"))
        .cloned()
        .and_then(|value| AppsConfigToml::deserialize(value).ok())
}

fn apply_requirements_apps_constraints(
    apps_config: &mut AppsConfigToml,
    requirements_apps_config: Option<&AppsRequirementsToml>,
) {
    let Some(requirements_apps_config) = requirements_apps_config else {
        return;
    };

    for (app_id, requirement) in &requirements_apps_config.apps {
        if requirement.enabled != Some(false) {
            continue;
        }
        let app = apps_config.apps.entry(app_id.clone()).or_default();
        app.enabled = false;
    }
}

fn app_is_enabled(apps_config: &AppsConfigToml, connector_id: Option<&str>) -> bool {
    let default_enabled = apps_config
        .default
        .as_ref()
        .map(|defaults| defaults.enabled)
        .unwrap_or(true);

    connector_id
        .and_then(|connector_id| apps_config.apps.get(connector_id))
        .map(|app| app.enabled)
        .unwrap_or(default_enabled)
}

fn app_tool_policy_from_apps_config(
    apps_config: Option<&AppsConfigToml>,
    connector_id: Option<&str>,
    tool_name: &str,
    tool_title: Option<&str>,
    annotations: Option<&ToolAnnotations>,
) -> AppToolPolicy {
    let Some(apps_config) = apps_config else {
        return AppToolPolicy::default();
    };

    let app = connector_id.and_then(|connector_id| apps_config.apps.get(connector_id));
    let tools = app.and_then(|app| app.tools.as_ref());
    let tool_config = tools.and_then(|tools| {
        tools
            .tools
            .get(tool_name)
            .or_else(|| tool_title.and_then(|title| tools.tools.get(title)))
    });
    let approval = tool_config
        .and_then(|tool| tool.approval_mode)
        .or_else(|| app.and_then(|app| app.default_tools_approval_mode))
        .unwrap_or(AppToolApproval::Auto);

    if !app_is_enabled(apps_config, connector_id) {
        return AppToolPolicy {
            enabled: false,
            approval,
        };
    }

    if let Some(enabled) = tool_config.and_then(|tool| tool.enabled) {
        return AppToolPolicy { enabled, approval };
    }

    if let Some(enabled) = app.and_then(|app| app.default_tools_enabled) {
        return AppToolPolicy { enabled, approval };
    }

    let app_defaults = apps_config.default.as_ref();
    let destructive_enabled = app
        .and_then(|app| app.destructive_enabled)
        .unwrap_or_else(|| {
            app_defaults
                .map(|defaults| defaults.destructive_enabled)
                .unwrap_or(true)
        });
    let open_world_enabled = app
        .and_then(|app| app.open_world_enabled)
        .unwrap_or_else(|| {
            app_defaults
                .map(|defaults| defaults.open_world_enabled)
                .unwrap_or(true)
        });
    let destructive_hint = annotations
        .and_then(|annotations| annotations.destructive_hint)
        .unwrap_or(true);
    let open_world_hint = annotations
        .and_then(|annotations| annotations.open_world_hint)
        .unwrap_or(true);
    let enabled =
        (destructive_enabled || !destructive_hint) && (open_world_enabled || !open_world_hint);

    AppToolPolicy { enabled, approval }
}

#[cfg(test)]
#[path = "connectors_tests.rs"]
mod tests;
