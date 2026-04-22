#![allow(clippy::unwrap_used, clippy::expect_used)]

use darwin_code_config::CONFIG_TOML_FILE;
use darwin_code_core::NewThread;
use darwin_code_features::Feature;
use darwin_code_login::DarwinCodeAuth;
use darwin_code_protocol::protocol::EventMsg;
use darwin_code_protocol::protocol::InitialHistory;
use darwin_code_protocol::protocol::WarningEvent;
use darwin_code_utils_absolute_path::AbsolutePathBuf;
use core::time::Duration;
use core_test_support::load_default_config_for_test;
use core_test_support::wait_for_event;
use tempfile::TempDir;
use tokio::time::timeout;
use toml::toml;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn emits_warning_when_unstable_features_enabled_via_config() {
    let home = TempDir::new().expect("tempdir");
    let mut config = load_default_config_for_test(&home).await;
    config
        .features
        .enable(Feature::ChildAgentsMd)
        .expect("test config should allow feature update");
    let user_config_path =
        AbsolutePathBuf::from_absolute_path(config.darwin_code_home.join(CONFIG_TOML_FILE))
            .expect("absolute user config path");
    config.config_layer_stack = config.config_layer_stack.with_user_config(
        &user_config_path,
        toml! { features = { child_agents_md = true } }.into(),
    );

    let thread_manager = darwin_code_core::test_support::thread_manager_with_models_provider(
        DarwinCodeAuth::from_api_key("test"),
        config.model_provider.clone(),
    );
    let auth_manager =
        darwin_code_core::test_support::auth_manager_from_auth(DarwinCodeAuth::from_api_key("test"));

    let NewThread {
        thread: conversation,
        ..
    } = thread_manager
        .resume_thread_with_history(
            config,
            InitialHistory::New,
            auth_manager,
            /*persist_extended_history*/ false,
            /*parent_trace*/ None,
        )
        .await
        .expect("spawn conversation");

    let warning = wait_for_event(&conversation, |ev| matches!(ev, EventMsg::Warning(_))).await;
    let EventMsg::Warning(WarningEvent { message }) = warning else {
        panic!("expected warning event");
    };
    assert!(message.contains("child_agents_md"));
    assert!(message.contains("Under-development features enabled"));
    assert!(message.contains("suppress_unstable_features_warning = true"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn suppresses_warning_when_configured() {
    let home = TempDir::new().expect("tempdir");
    let mut config = load_default_config_for_test(&home).await;
    config
        .features
        .enable(Feature::ChildAgentsMd)
        .expect("test config should allow feature update");
    config.suppress_unstable_features_warning = true;
    let user_config_path =
        AbsolutePathBuf::from_absolute_path(config.darwin_code_home.join(CONFIG_TOML_FILE))
            .expect("absolute user config path");
    config.config_layer_stack = config.config_layer_stack.with_user_config(
        &user_config_path,
        toml! { features = { child_agents_md = true } }.into(),
    );

    let thread_manager = darwin_code_core::test_support::thread_manager_with_models_provider(
        DarwinCodeAuth::from_api_key("test"),
        config.model_provider.clone(),
    );
    let auth_manager =
        darwin_code_core::test_support::auth_manager_from_auth(DarwinCodeAuth::from_api_key("test"));

    let NewThread {
        thread: conversation,
        ..
    } = thread_manager
        .resume_thread_with_history(
            config,
            InitialHistory::New,
            auth_manager,
            /*persist_extended_history*/ false,
            /*parent_trace*/ None,
        )
        .await
        .expect("spawn conversation");

    let warning = timeout(
        Duration::from_millis(150),
        wait_for_event(&conversation, |ev| matches!(ev, EventMsg::Warning(_))),
    )
    .await;
    assert!(warning.is_err());
}
