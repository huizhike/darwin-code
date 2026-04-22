use super::*;
use crate::plugins::PluginInstallRequest;
use crate::plugins::PluginsManager;
use crate::plugins::test_support::load_plugins_config;
use crate::plugins::test_support::write_curated_plugin_sha;
use crate::plugins::test_support::write_openai_curated_marketplace;
use crate::plugins::test_support::write_plugins_feature_config;
use darwin_code_utils_absolute_path::AbsolutePathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn verified_plugin_suggestion_completed_requires_installed_plugin() {
    let darwin_code_home = tempdir().expect("tempdir should succeed");
    let curated_root = crate::plugins::curated_plugins_repo_path(darwin_code_home.path());
    write_openai_curated_marketplace(&curated_root, &["sample"]);
    write_curated_plugin_sha(darwin_code_home.path());
    write_plugins_feature_config(darwin_code_home.path());

    let config = load_plugins_config(darwin_code_home.path()).await;
    let plugins_manager = PluginsManager::new(darwin_code_home.path().to_path_buf());

    assert!(!verified_plugin_suggestion_completed(
        "sample@openai-curated",
        &config,
        &plugins_manager,
    ));

    plugins_manager
        .install_plugin(PluginInstallRequest {
            plugin_name: "sample".to_string(),
            marketplace_path: AbsolutePathBuf::try_from(
                curated_root.join(".agents/plugins/marketplace.json"),
            )
            .expect("marketplace path"),
        })
        .await
        .expect("plugin should install");

    let refreshed_config = load_plugins_config(darwin_code_home.path()).await;
    assert!(verified_plugin_suggestion_completed(
        "sample@openai-curated",
        &refreshed_config,
        &plugins_manager,
    ));
}
