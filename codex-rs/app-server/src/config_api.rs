use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use async_trait::async_trait;
use codex_analytics::AnalyticsEventsClient;
use darwin_code_app_server_protocol::ConfigBatchWriteParams;
use darwin_code_app_server_protocol::ConfigReadParams;
use darwin_code_app_server_protocol::ConfigReadResponse;
use darwin_code_app_server_protocol::ConfigValueWriteParams;
use darwin_code_app_server_protocol::ConfigWriteErrorCode;
use darwin_code_app_server_protocol::ConfigWriteResponse;
use darwin_code_app_server_protocol::ExperimentalFeatureEnablementSetParams;
use darwin_code_app_server_protocol::ExperimentalFeatureEnablementSetResponse;
use darwin_code_app_server_protocol::JSONRPCErrorError;
use darwin_code_core::ThreadManager;
use darwin_code_core::config::Config;
use darwin_code_core::config::ConfigService;
use darwin_code_core::config::ConfigServiceError;
use darwin_code_core::config_loader::ExternalRequirementsLoader;
use darwin_code_core::config_loader::LoaderOverrides;
use darwin_code_core::plugins::PluginId;
use darwin_code_core_plugins::loader::installed_plugin_telemetry_metadata;
use darwin_code_core_plugins::toggles::collect_plugin_enabled_candidates;
use darwin_code_features::canonical_feature_for_key;
use darwin_code_features::feature_for_key;
use darwin_code_protocol::protocol::Op;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use toml::Value as TomlValue;
use tracing::warn;

const SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT: &[&str] = &[
    "apps",
    "plugins",
    "tool_search",
    "tool_suggest",
    "tool_call_mcp_elicitation",
];

#[async_trait]
pub(crate) trait UserConfigReloader: Send + Sync {
    async fn reload_user_config(&self);
}

#[async_trait]
impl UserConfigReloader for ThreadManager {
    async fn reload_user_config(&self) {
        let thread_ids = self.list_thread_ids().await;
        for thread_id in thread_ids {
            let Ok(thread) = self.get_thread(thread_id).await else {
                continue;
            };
            if let Err(err) = thread.submit(Op::ReloadUserConfig).await {
                warn!("failed to request user config reload: {err}");
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct ConfigApi {
    darwin_code_home: PathBuf,
    cli_overrides: Arc<RwLock<Vec<(String, TomlValue)>>>,
    runtime_feature_enablement: Arc<RwLock<BTreeMap<String, bool>>>,
    loader_overrides: LoaderOverrides,
    external_requirements: Arc<RwLock<ExternalRequirementsLoader>>,
    user_config_reloader: Arc<dyn UserConfigReloader>,
    analytics_events_client: AnalyticsEventsClient,
}

impl ConfigApi {
    pub(crate) fn new(
        darwin_code_home: PathBuf,
        cli_overrides: Arc<RwLock<Vec<(String, TomlValue)>>>,
        runtime_feature_enablement: Arc<RwLock<BTreeMap<String, bool>>>,
        loader_overrides: LoaderOverrides,
        external_requirements: Arc<RwLock<ExternalRequirementsLoader>>,
        user_config_reloader: Arc<dyn UserConfigReloader>,
        analytics_events_client: AnalyticsEventsClient,
    ) -> Self {
        Self {
            darwin_code_home,
            cli_overrides,
            runtime_feature_enablement,
            loader_overrides,
            external_requirements,
            user_config_reloader,
            analytics_events_client,
        }
    }

    fn config_service(&self) -> ConfigService {
        ConfigService::new(
            self.darwin_code_home.clone(),
            self.current_cli_overrides(),
            self.loader_overrides.clone(),
            self.current_external_requirements(),
        )
    }

    fn current_cli_overrides(&self) -> Vec<(String, TomlValue)> {
        self.cli_overrides
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn current_runtime_feature_enablement(&self) -> BTreeMap<String, bool> {
        self.runtime_feature_enablement
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn current_external_requirements(&self) -> ExternalRequirementsLoader {
        self.external_requirements
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub(crate) async fn load_latest_config(
        &self,
        fallback_cwd: Option<PathBuf>,
    ) -> Result<Config, JSONRPCErrorError> {
        let mut config = darwin_code_core::config::ConfigBuilder::default()
            .darwin_code_home(self.darwin_code_home.clone())
            .cli_overrides(self.current_cli_overrides())
            .loader_overrides(self.loader_overrides.clone())
            .fallback_cwd(fallback_cwd)
            .external_requirements(self.current_external_requirements())
            .build()
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to resolve feature override precedence: {err}"),
                data: None,
            })?;
        apply_runtime_feature_enablement(&mut config, &self.current_runtime_feature_enablement());
        Ok(config)
    }

    pub(crate) async fn read(
        &self,
        params: ConfigReadParams,
    ) -> Result<ConfigReadResponse, JSONRPCErrorError> {
        let fallback_cwd = params.cwd.as_ref().map(PathBuf::from);
        let mut response = self
            .config_service()
            .read(params)
            .await
            .map_err(map_error)?;
        let config = self.load_latest_config(fallback_cwd).await?;
        for feature_key in SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT {
            let Some(feature) = feature_for_key(feature_key) else {
                continue;
            };
            let features = response
                .config
                .additional
                .entry("features".to_string())
                .or_insert_with(|| json!({}));
            if !features.is_object() {
                *features = json!({});
            }
            if let Some(features) = features.as_object_mut() {
                features.insert(
                    (*feature_key).to_string(),
                    json!(config.features.enabled(feature)),
                );
            }
        }
        Ok(response)
    }
    pub(crate) async fn write_value(
        &self,
        params: ConfigValueWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        let pending_changes =
            collect_plugin_enabled_candidates([(&params.key_path, &params.value)].into_iter());
        let response = self
            .config_service()
            .write_value(params)
            .await
            .map_err(map_error)?;
        self.emit_plugin_toggle_events(pending_changes).await;
        Ok(response)
    }

    pub(crate) async fn batch_write(
        &self,
        params: ConfigBatchWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        let reload_user_config = params.reload_user_config;
        let pending_changes = collect_plugin_enabled_candidates(
            params
                .edits
                .iter()
                .map(|edit| (&edit.key_path, &edit.value)),
        );
        let response = self
            .config_service()
            .batch_write(params)
            .await
            .map_err(map_error)?;
        self.emit_plugin_toggle_events(pending_changes).await;
        if reload_user_config {
            self.user_config_reloader.reload_user_config().await;
        }
        Ok(response)
    }

    pub(crate) async fn set_experimental_feature_enablement(
        &self,
        params: ExperimentalFeatureEnablementSetParams,
    ) -> Result<ExperimentalFeatureEnablementSetResponse, JSONRPCErrorError> {
        let ExperimentalFeatureEnablementSetParams { enablement } = params;
        for key in enablement.keys() {
            if canonical_feature_for_key(key).is_some() {
                if SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT.contains(&key.as_str()) {
                    continue;
                }

                return Err(JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!(
                        "unsupported feature enablement `{key}`: currently supported features are {}",
                        SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT.join(", ")
                    ),
                    data: None,
                });
            }

            let message = if let Some(feature) = feature_for_key(key) {
                format!(
                    "invalid feature enablement `{key}`: use canonical feature key `{}`",
                    feature.key()
                )
            } else {
                format!("invalid feature enablement `{key}`")
            };
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message,
                data: None,
            });
        }

        if enablement.is_empty() {
            return Ok(ExperimentalFeatureEnablementSetResponse { enablement });
        }

        {
            let mut runtime_feature_enablement =
                self.runtime_feature_enablement
                    .write()
                    .map_err(|_| JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: "failed to update feature enablement".to_string(),
                        data: None,
                    })?;
            runtime_feature_enablement.extend(
                enablement
                    .iter()
                    .map(|(name, enabled)| (name.clone(), *enabled)),
            );
        }

        self.load_latest_config(/*fallback_cwd*/ None).await?;
        self.user_config_reloader.reload_user_config().await;

        Ok(ExperimentalFeatureEnablementSetResponse { enablement })
    }

    async fn emit_plugin_toggle_events(
        &self,
        pending_changes: std::collections::BTreeMap<String, bool>,
    ) {
        for (plugin_id, enabled) in pending_changes {
            let Ok(plugin_id) = PluginId::parse(&plugin_id) else {
                continue;
            };
            let metadata =
                installed_plugin_telemetry_metadata(self.darwin_code_home.as_path(), &plugin_id)
                    .await;
            if enabled {
                self.analytics_events_client.track_plugin_enabled(metadata);
            } else {
                self.analytics_events_client.track_plugin_disabled(metadata);
            }
        }
    }
}

pub(crate) fn protected_feature_keys(
    config_layer_stack: &darwin_code_core::config_loader::ConfigLayerStack,
) -> BTreeSet<String> {
    let mut protected_features = config_layer_stack
        .effective_config()
        .get("features")
        .and_then(toml::Value::as_table)
        .map(|features| features.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();

    if let Some(feature_requirements) = config_layer_stack
        .requirements_toml()
        .feature_requirements
        .as_ref()
    {
        protected_features.extend(feature_requirements.entries.keys().cloned());
    }

    protected_features
}

pub(crate) fn apply_runtime_feature_enablement(
    config: &mut Config,
    runtime_feature_enablement: &BTreeMap<String, bool>,
) {
    let protected_features = protected_feature_keys(&config.config_layer_stack);
    for (name, enabled) in runtime_feature_enablement {
        if protected_features.contains(name) {
            continue;
        }
        let Some(feature) = feature_for_key(name) else {
            continue;
        };
        if let Err(err) = config.features.set_enabled(feature, *enabled) {
            warn!(
                feature = name,
                error = %err,
                "failed to apply runtime feature enablement"
            );
        }
    }
}

fn map_error(err: ConfigServiceError) -> JSONRPCErrorError {
    if let Some(code) = err.write_error_code() {
        return config_write_error(code, err.to_string());
    }

    JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: err.to_string(),
        data: None,
    }
}

fn config_write_error(code: ConfigWriteErrorCode, message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: message.into(),
        data: Some(json!({
            "config_write_error_code": code,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darwin_code_core::config_loader::ConfigRequirementsToml;
    use darwin_code_features::Feature;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;

    const TEST_BYOK_PROVIDER_CONFIG: &str = r#"
[providers.openai]
family = "openai-compatible"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
api_key = "test-direct-api-key"
"#;

    #[derive(Default)]
    struct RecordingUserConfigReloader {
        call_count: AtomicUsize,
    }

    #[async_trait]
    impl UserConfigReloader for RecordingUserConfigReloader {
        async fn reload_user_config(&self) {
            self.call_count.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[tokio::test]
    async fn apply_runtime_feature_enablement_keeps_cli_overrides_above_config_and_runtime() {
        let darwin_code_home = TempDir::new().expect("create temp dir");
        std::fs::write(
            darwin_code_home.path().join("config.toml"),
            format!("{TEST_BYOK_PROVIDER_CONFIG}\n[features]\napps = false\n"),
        )
        .expect("write config");

        let mut config = darwin_code_core::config::ConfigBuilder::default()
            .darwin_code_home(darwin_code_home.path().to_path_buf())
            .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
            .cli_overrides(vec![(
                "features.apps".to_string(),
                TomlValue::Boolean(true),
            )])
            .build()
            .await
            .expect("load config");

        apply_runtime_feature_enablement(
            &mut config,
            &BTreeMap::from([("apps".to_string(), false)]),
        );

        assert!(config.features.enabled(Feature::Apps));
    }

    #[tokio::test]
    async fn apply_runtime_feature_enablement_keeps_cloud_pins_above_cli_and_runtime() {
        let darwin_code_home = TempDir::new().expect("create temp dir");
        std::fs::write(
            darwin_code_home.path().join("config.toml"),
            TEST_BYOK_PROVIDER_CONFIG,
        )
        .expect("write config");

        let mut config = darwin_code_core::config::ConfigBuilder::default()
            .darwin_code_home(darwin_code_home.path().to_path_buf())
            .cli_overrides(vec![(
                "features.apps".to_string(),
                TomlValue::Boolean(true),
            )])
            .external_requirements(ExternalRequirementsLoader::new(async {
                Ok(Some(ConfigRequirementsToml {
                    feature_requirements: Some(
                        darwin_code_core::config_loader::FeatureRequirementsToml {
                            entries: BTreeMap::from([("apps".to_string(), false)]),
                        },
                    ),
                    ..Default::default()
                }))
            }))
            .build()
            .await
            .expect("load config");

        apply_runtime_feature_enablement(
            &mut config,
            &BTreeMap::from([("apps".to_string(), true)]),
        );

        assert!(!config.features.enabled(Feature::Apps));
    }

    #[tokio::test]
    async fn batch_write_reloads_user_config_when_requested() {
        let darwin_code_home = TempDir::new().expect("create temp dir");
        let user_config_path = darwin_code_home.path().join("config.toml");
        std::fs::write(&user_config_path, "").expect("write config");
        let reloader = Arc::new(RecordingUserConfigReloader::default());
        let analytics_config = Arc::new(
            darwin_code_core::config::ConfigBuilder::default()
                .build()
                .await
                .expect("load analytics config"),
        );
        let config_api = ConfigApi::new(
            darwin_code_home.path().to_path_buf(),
            Arc::new(RwLock::new(Vec::new())),
            Arc::new(RwLock::new(BTreeMap::new())),
            LoaderOverrides::default(),
            Arc::new(RwLock::new(ExternalRequirementsLoader::default())),
            reloader.clone(),
            AnalyticsEventsClient::new(analytics_config.analytics_enabled),
        );

        let response = config_api
            .batch_write(ConfigBatchWriteParams {
                edits: vec![darwin_code_app_server_protocol::ConfigEdit {
                    key_path: "model".to_string(),
                    value: json!("gpt-5"),
                    merge_strategy: darwin_code_app_server_protocol::MergeStrategy::Replace,
                }],
                file_path: Some(user_config_path.display().to_string()),
                expected_version: None,
                reload_user_config: true,
            })
            .await
            .expect("batch write should succeed");

        assert_eq!(
            response,
            ConfigWriteResponse {
                status: darwin_code_app_server_protocol::WriteStatus::Ok,
                version: response.version.clone(),
                file_path: darwin_code_utils_absolute_path::AbsolutePathBuf::try_from(
                    user_config_path.clone()
                )
                .expect("absolute config path"),
                overridden_metadata: None,
            }
        );
        assert_eq!(
            std::fs::read_to_string(user_config_path).unwrap(),
            "model = \"gpt-5\"\n"
        );
        assert_eq!(reloader.call_count.load(Ordering::Relaxed), 1);
    }
}
