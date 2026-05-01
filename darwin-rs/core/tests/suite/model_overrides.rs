use core_test_support::responses::start_mock_server;
use core_test_support::test_darwin_code::test_darwin_code;
use core_test_support::wait_for_event;
use darwin_code_protocol::model_metadata::ReasoningEffort;
use darwin_code_protocol::protocol::EventMsg;
use darwin_code_protocol::protocol::Op;
use pretty_assertions::assert_eq;

const CONFIG_TOML: &str = "config.toml";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn override_turn_context_does_not_persist_when_config_exists() {
    let server = start_mock_server().await;
    let initial_contents = "model = \"gpt-4o\"\n";
    let mut builder = test_darwin_code()
        .with_pre_build_hook(move |home| {
            let config_path = home.join(CONFIG_TOML);
            std::fs::write(config_path, initial_contents).expect("seed config.toml");
        })
        .with_config(|config| {
            config.model = Some("gpt-4o".to_string());
        });
    let test = builder.build(&server).await.expect("create conversation");
    let darwin_code = test.darwin_code.clone();
    let config_path = test.home.path().join(CONFIG_TOML);

    darwin_code
        .submit(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model: Some("o3".to_string()),
            effort: Some(Some(ReasoningEffort::High)),
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await
        .expect("submit override");

    darwin_code
        .submit(Op::Shutdown)
        .await
        .expect("request shutdown");
    wait_for_event(&darwin_code, |ev| matches!(ev, EventMsg::ShutdownComplete)).await;

    let contents = tokio::fs::read_to_string(&config_path)
        .await
        .expect("read config.toml after override");
    assert_eq!(contents, initial_contents);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn override_turn_context_does_not_create_config_file() {
    let server = start_mock_server().await;
    let mut builder = test_darwin_code();
    let test = builder.build(&server).await.expect("create conversation");
    let darwin_code = test.darwin_code.clone();
    let config_path = test.home.path().join(CONFIG_TOML);
    let initial_contents = tokio::fs::read_to_string(&config_path)
        .await
        .expect("BYOK test bootstrap should seed config.toml");

    darwin_code
        .submit(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model: Some("o3".to_string()),
            effort: Some(Some(ReasoningEffort::Medium)),
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await
        .expect("submit override");

    darwin_code
        .submit(Op::Shutdown)
        .await
        .expect("request shutdown");
    wait_for_event(&darwin_code, |ev| matches!(ev, EventMsg::ShutdownComplete)).await;

    let contents = tokio::fs::read_to_string(&config_path)
        .await
        .expect("read config.toml after override");
    assert_eq!(contents, initial_contents);
}
