use crate::config::Config;
use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::config_loader::ConfigLayerStack;
use darwin_code_analytics::AnalyticsEventsClient;
use darwin_code_core_plugins::loader::installed_plugin_telemetry_metadata;
use darwin_code_core_plugins::loader::load_plugins_from_layer_stack;
use darwin_code_core_plugins::loader::log_plugin_load_errors;
use darwin_code_core_plugins::store::PluginStore;
use darwin_code_core_plugins::store::PluginStoreError;
use darwin_code_features::Feature;
use darwin_code_plugin::PluginId;
use darwin_code_plugin::PluginIdError;
use darwin_code_protocol::protocol::Product;
use darwin_code_utils_absolute_path::AbsolutePathBuf;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;

use super::PluginLoadOutcome;

pub struct PluginsManager {
    darwin_code_home: PathBuf,
    store: PluginStore,
    cached_enabled_outcome: RwLock<Option<PluginLoadOutcome>>,
    restriction_product: Option<Product>,
    analytics_events_client: RwLock<Option<AnalyticsEventsClient>>,
}

impl PluginsManager {
    pub fn new(darwin_code_home: PathBuf) -> Self {
        Self::new_with_restriction_product(darwin_code_home, Some(Product::DarwinCode))
    }

    pub fn new_with_restriction_product(
        darwin_code_home: PathBuf,
        restriction_product: Option<Product>,
    ) -> Self {
        Self {
            darwin_code_home: darwin_code_home.clone(),
            store: PluginStore::new(darwin_code_home),
            cached_enabled_outcome: RwLock::new(None),
            restriction_product,
            analytics_events_client: RwLock::new(None),
        }
    }

    pub fn set_analytics_events_client(&self, analytics_events_client: AnalyticsEventsClient) {
        let mut stored_client = match self.analytics_events_client.write() {
            Ok(client_guard) => client_guard,
            Err(err) => err.into_inner(),
        };
        *stored_client = Some(analytics_events_client);
    }

    pub async fn plugins_for_config(&self, config: &Config) -> PluginLoadOutcome {
        self.plugins_for_config_with_force_reload(config, /*force_reload*/ false)
            .await
    }

    pub(crate) async fn plugins_for_config_with_force_reload(
        &self,
        config: &Config,
        force_reload: bool,
    ) -> PluginLoadOutcome {
        if !config.features.enabled(Feature::Plugins) {
            return PluginLoadOutcome::default();
        }

        if !force_reload && let Some(outcome) = self.cached_enabled_outcome() {
            return outcome;
        }

        let outcome = load_plugins_from_layer_stack(
            &config.config_layer_stack,
            &self.store,
            self.restriction_product,
        )
        .await;
        log_plugin_load_errors(&outcome);
        let mut cache = match self.cached_enabled_outcome.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *cache = Some(outcome.clone());
        outcome
    }

    pub fn clear_cache(&self) {
        let mut cached_enabled_outcome = match self.cached_enabled_outcome.write() {
            Ok(cache) => cache,
            Err(err) => err.into_inner(),
        };
        *cached_enabled_outcome = None;
    }

    /// Resolve plugin skill roots for a config layer stack without touching the plugins cache.
    pub async fn effective_skill_roots_for_layer_stack(
        &self,
        config_layer_stack: &ConfigLayerStack,
        plugins_feature_enabled: bool,
    ) -> Vec<AbsolutePathBuf> {
        if !plugins_feature_enabled {
            return Vec::new();
        }
        load_plugins_from_layer_stack(config_layer_stack, &self.store, self.restriction_product)
            .await
            .effective_skill_roots()
    }

    fn cached_enabled_outcome(&self) -> Option<PluginLoadOutcome> {
        match self.cached_enabled_outcome.read() {
            Ok(cache) => cache.clone(),
            Err(err) => err.into_inner().clone(),
        }
    }

    /// Plugin startup/refresh acquisition tasks were removed. Keep this method as a no-op so callers can
    /// remain agnostic to whether optional plugin acquisition surfaces are compiled in.
    pub fn maybe_start_plugin_startup_tasks_for_config(self: &Arc<Self>, _config: &Config) {}

    pub async fn uninstall_plugin(&self, plugin_id: String) -> Result<(), PluginUninstallError> {
        let plugin_id = PluginId::parse(&plugin_id)?;
        self.uninstall_plugin_id(plugin_id).await
    }

    async fn uninstall_plugin_id(&self, plugin_id: PluginId) -> Result<(), PluginUninstallError> {
        let plugin_telemetry = if self.store.active_plugin_root(&plugin_id).is_some() {
            Some(
                installed_plugin_telemetry_metadata(self.darwin_code_home.as_path(), &plugin_id)
                    .await,
            )
        } else {
            None
        };
        let store = self.store.clone();
        let plugin_id_for_store = plugin_id.clone();
        tokio::task::spawn_blocking(move || store.uninstall(&plugin_id_for_store))
            .await
            .map_err(PluginUninstallError::join)??;

        ConfigEditsBuilder::new(&self.darwin_code_home)
            .with_edits([ConfigEdit::ClearPath {
                segments: vec!["plugins".to_string(), plugin_id.as_key()],
            }])
            .apply()
            .await?;

        let analytics_events_client = match self.analytics_events_client.read() {
            Ok(client) => client.clone(),
            Err(err) => err.into_inner().clone(),
        };
        if let Some(plugin_telemetry) = plugin_telemetry
            && let Some(analytics_events_client) = analytics_events_client
        {
            analytics_events_client.track_plugin_uninstalled(plugin_telemetry);
        }

        self.clear_cache();
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginUninstallError {
    #[error("{0}")]
    InvalidPluginId(#[from] PluginIdError),

    #[error("{0}")]
    Store(#[from] PluginStoreError),

    #[error("{0}")]
    Config(#[from] anyhow::Error),

    #[error("failed to join plugin uninstall task: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl PluginUninstallError {
    fn join(source: tokio::task::JoinError) -> Self {
        Self::Join(source)
    }

    pub fn is_invalid_request(&self) -> bool {
        matches!(self, Self::InvalidPluginId(_))
    }
}
