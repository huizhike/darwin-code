use darwin_code_protocol::protocol::Product;
use serde::Deserialize;

const DEFAULT_REMOTE_MARKETPLACE_NAME: &str = "openai-curated";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RemotePluginServiceConfig {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemotePluginStatusSummary {
    pub name: String,
    #[serde(default = "default_remote_marketplace_name")]
    pub marketplace_name: String,
    pub enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RemotePluginMutationError {
    #[error("remote plugin mutation is unavailable in BYOK-only DarwinCode")]
    UnavailableInByokMode,
}

#[derive(Debug, thiserror::Error)]
pub enum RemotePluginFetchError {
    #[error("remote plugin sync is unavailable in BYOK-only DarwinCode")]
    UnavailableInByokMode,
}

pub async fn fetch_remote_plugin_status(
    _config: &RemotePluginServiceConfig,
) -> Result<Vec<RemotePluginStatusSummary>, RemotePluginFetchError> {
    Ok(Vec::new())
}

pub async fn fetch_remote_featured_plugin_ids(
    _config: &RemotePluginServiceConfig,
    _product: Option<Product>,
) -> Result<Vec<String>, RemotePluginFetchError> {
    Ok(Vec::new())
}

pub async fn enable_remote_plugin(
    _config: &RemotePluginServiceConfig,
    _plugin_id: &str,
) -> Result<(), RemotePluginMutationError> {
    Err(RemotePluginMutationError::UnavailableInByokMode)
}

pub async fn uninstall_remote_plugin(
    _config: &RemotePluginServiceConfig,
    _plugin_id: &str,
) -> Result<(), RemotePluginMutationError> {
    Err(RemotePluginMutationError::UnavailableInByokMode)
}

fn default_remote_marketplace_name() -> String {
    DEFAULT_REMOTE_MARKETPLACE_NAME.to_string()
}
