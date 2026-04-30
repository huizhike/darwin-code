//! Test-only helpers exposed for cross-crate integration tests.
//!
//! Production code should not depend on this module.
//! We prefer this to using a crate feature to avoid building multiple
//! permutations of the crate.

use std::path::PathBuf;
use std::sync::Arc;

use darwin_code_exec_server::EnvironmentManager;
use darwin_code_model_provider_info::ModelProviderInfo;
use darwin_code_models_manager::bundled_models_response;
use darwin_code_models_manager::collaboration_mode_presets;
use darwin_code_models_manager::manager::ModelsManager;
use darwin_code_protocol::config_types::CollaborationModeMask;
use darwin_code_protocol::openai_models::ModelInfo;
use darwin_code_protocol::openai_models::ModelPreset;
use once_cell::sync::Lazy;

use crate::ThreadManager;
use crate::config::Config;
use crate::thread_manager;
use crate::unified_exec;

#[derive(Clone, Debug)]
pub struct ByokTestAuth;

impl ByokTestAuth {
    pub fn from_api_key(_api_key: impl Into<String>) -> Self {
        Self
    }

    pub fn dummy_for_testing() -> Self {
        Self
    }
}

pub const DARWIN_CODE_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
pub const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";

static TEST_MODEL_PRESETS: Lazy<Vec<ModelPreset>> = Lazy::new(|| {
    let mut response = bundled_models_response()
        .unwrap_or_else(|err| panic!("bundled models.json should parse: {err}"));
    response.models.sort_by(|a, b| a.priority.cmp(&b.priority));
    let mut presets: Vec<ModelPreset> = response.models.into_iter().map(Into::into).collect();
    ModelPreset::mark_default_by_picker_visibility(&mut presets);
    presets
});

pub fn set_thread_manager_test_mode(enabled: bool) {
    thread_manager::set_thread_manager_test_mode_for_tests(enabled);
}

pub fn set_deterministic_process_ids(enabled: bool) {
    unified_exec::set_deterministic_process_ids_for_tests(enabled);
}

pub fn thread_manager_with_models_provider<T>(
    auth: T,
    provider: ModelProviderInfo,
) -> ThreadManager {
    ThreadManager::with_models_provider_for_tests(auth, provider)
}

pub fn thread_manager_with_models_provider_and_home<T>(
    auth: T,
    provider: ModelProviderInfo,
    darwin_code_home: PathBuf,
    environment_manager: Arc<EnvironmentManager>,
) -> ThreadManager {
    ThreadManager::with_models_provider_and_home_for_tests(
        auth,
        provider,
        darwin_code_home,
        environment_manager,
    )
}

pub async fn start_thread_with_user_shell_override(
    thread_manager: &ThreadManager,
    config: Config,
    user_shell_override: crate::shell::Shell,
) -> darwin_code_protocol::error::Result<crate::NewThread> {
    thread_manager
        .start_thread_with_user_shell_override_for_tests(config, user_shell_override)
        .await
}

pub async fn resume_thread_from_rollout_with_user_shell_override(
    thread_manager: &ThreadManager,
    config: Config,
    rollout_path: PathBuf,
    user_shell_override: crate::shell::Shell,
) -> darwin_code_protocol::error::Result<crate::NewThread> {
    thread_manager
        .resume_thread_from_rollout_with_user_shell_override_for_tests(
            config,
            rollout_path,
            user_shell_override,
        )
        .await
}

pub fn models_manager_with_provider(
    darwin_code_home: PathBuf,
    provider: ModelProviderInfo,
) -> ModelsManager {
    ModelsManager::with_provider_for_tests(darwin_code_home, provider)
}

pub fn get_model_offline(model: Option<&str>) -> String {
    ModelsManager::get_model_offline_for_tests(model)
}

pub fn construct_model_info_offline(model: &str, config: &Config) -> ModelInfo {
    ModelsManager::construct_model_info_offline_for_tests(model, &config.to_models_manager_config())
}

pub fn all_model_presets() -> &'static Vec<ModelPreset> {
    &TEST_MODEL_PRESETS
}

pub fn builtin_collaboration_mode_presets() -> Vec<CollaborationModeMask> {
    collaboration_mode_presets::builtin_collaboration_mode_presets(
        collaboration_mode_presets::CollaborationModesConfig::default(),
    )
}
