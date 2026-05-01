use darwin_code_config::types::McpServerConfig;

mod injection;
mod manager;
mod mentions;
mod render;

pub use darwin_code_plugin::AppConnectorId;
pub use darwin_code_plugin::EffectiveSkillRoots;
pub use darwin_code_plugin::PluginCapabilitySummary;
pub use darwin_code_plugin::PluginId;
pub use darwin_code_plugin::PluginIdError;
pub use darwin_code_plugin::PluginTelemetryMetadata;
pub use darwin_code_plugin::validate_plugin_segment;

pub type LoadedPlugin = darwin_code_plugin::LoadedPlugin<McpServerConfig>;
pub type PluginLoadOutcome = darwin_code_plugin::PluginLoadOutcome<McpServerConfig>;

pub(crate) use injection::build_plugin_injections;
pub use manager::PluginUninstallError;
pub use manager::PluginsManager;
pub(crate) use render::render_explicit_plugin_instructions;
pub(crate) use render::render_plugins_section;

pub(crate) use mentions::build_connector_slug_counts;
pub(crate) use mentions::build_skill_name_counts;
pub(crate) use mentions::collect_explicit_app_ids;
pub(crate) use mentions::collect_explicit_plugin_mentions;
pub(crate) use mentions::collect_tool_mentions_from_messages;
