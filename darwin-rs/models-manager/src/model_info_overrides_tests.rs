use crate::ModelsManagerConfig;
use crate::collaboration_mode_presets::CollaborationModesConfig;
use crate::manager::ModelsManager;
use darwin_code_protocol::model_metadata::TruncationPolicyConfig;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_model_info_without_tool_output_override() {
    let darwin_code_home = TempDir::new().expect("create temp dir");
    let config = ModelsManagerConfig::default();
    let manager = ModelsManager::new(
        darwin_code_home.path().to_path_buf(),
        /*model_catalog*/ None,
        CollaborationModesConfig::default(),
    );

    let model_info = manager.get_model_info("gpt-5.1", &config).await;

    assert_eq!(
        model_info.truncation_policy,
        TruncationPolicyConfig::bytes(/*limit*/ 10_000)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn offline_model_info_with_tool_output_override() {
    let darwin_code_home = TempDir::new().expect("create temp dir");
    let config = ModelsManagerConfig {
        tool_output_token_limit: Some(123),
        ..Default::default()
    };
    let manager = ModelsManager::new(
        darwin_code_home.path().to_path_buf(),
        /*model_catalog*/ None,
        CollaborationModesConfig::default(),
    );

    let model_info = manager.get_model_info("gpt-5.1-darwin-code", &config).await;

    assert_eq!(
        model_info.truncation_policy,
        TruncationPolicyConfig::tokens(/*limit*/ 123)
    );
}
