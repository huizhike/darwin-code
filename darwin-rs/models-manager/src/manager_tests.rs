use super::*;
use crate::ModelsManagerConfig;
use chrono::Utc;
use core_test_support::responses::mount_models_once;
use darwin_code_model_provider_info::WireApi;
use darwin_code_protocol::model_metadata::ModelsResponse;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::tempdir;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header_regex;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[path = "model_info_overrides_tests.rs"]
mod model_info_overrides_tests;

fn remote_model(slug: &str, display: &str, priority: i32) -> ModelInfo {
    remote_model_with_visibility(slug, display, priority, "list")
}

fn remote_model_with_visibility(
    slug: &str,
    display: &str,
    priority: i32,
    visibility: &str,
) -> ModelInfo {
    serde_json::from_value(json!({
            "slug": slug,
            "display_name": display,
            "description": format!("{display} desc"),
            "default_reasoning_level": "medium",
            "supported_reasoning_levels": [{"effort": "low", "description": "low"}, {"effort": "medium", "description": "medium"}],
            "shell_type": "shell_command",
            "visibility": visibility,
            "minimal_client_version": [0, 1, 0],
            "supported_in_api": true,
            "priority": priority,
            "upgrade": null,
            "base_instructions": "base instructions",
            "supports_reasoning_summaries": false,
            "support_verbosity": false,
            "default_verbosity": null,
            "apply_patch_tool_type": null,
            "truncation_policy": {"mode": "bytes", "limit": 10_000},
            "supports_parallel_tool_calls": false,
            "supports_image_detail_original": false,
            "context_window": 272_000,
            "experimental_supported_tools": [],
        }))
        .expect("valid model")
}

fn assert_models_contain(actual: &[ModelInfo], expected: &[ModelInfo]) {
    for model in expected {
        assert!(
            actual.iter().any(|candidate| candidate.slug == model.slug),
            "expected model {} in cached list",
            model.slug
        );
    }
}

fn provider_for(base_url: String) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "mock".into(),
        base_url: Some(base_url),
        api_key: None,
        api_key_env: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
    }
}

#[tokio::test]
async fn get_model_info_tracks_fallback_usage() {
    let darwin_code_home = tempdir().expect("temp dir");
    let config = ModelsManagerConfig::default();
    let manager = ModelsManager::new(
        darwin_code_home.path().to_path_buf(),
        /*model_catalog*/ None,
        CollaborationModesConfig::default(),
    );
    let known_slug = manager
        .get_remote_models()
        .await
        .first()
        .expect("bundled models should include at least one model")
        .slug
        .clone();

    let known = manager.get_model_info(known_slug.as_str(), &config).await;
    assert!(!known.used_fallback_model_metadata);
    assert_eq!(known.slug, known_slug);

    let unknown = manager
        .get_model_info("model-that-does-not-exist", &config)
        .await;
    assert!(unknown.used_fallback_model_metadata);
    assert_eq!(unknown.slug, "model-that-does-not-exist");
}

#[tokio::test]
async fn get_model_info_uses_custom_catalog() {
    let darwin_code_home = tempdir().expect("temp dir");
    let config = ModelsManagerConfig::default();
    let mut overlay = remote_model("gpt-overlay", "Overlay", /*priority*/ 0);
    overlay.supports_image_detail_original = true;

    let manager = ModelsManager::new(
        darwin_code_home.path().to_path_buf(),
        Some(ModelsResponse {
            models: vec![overlay],
        }),
        CollaborationModesConfig::default(),
    );

    let model_info = manager
        .get_model_info("gpt-overlay-experiment", &config)
        .await;

    assert_eq!(model_info.slug, "gpt-overlay-experiment");
    assert_eq!(model_info.display_name, "Overlay");
    assert_eq!(model_info.context_window, Some(272_000));
    assert!(model_info.supports_image_detail_original);
    assert!(!model_info.supports_parallel_tool_calls);
    assert!(!model_info.used_fallback_model_metadata);
}

#[tokio::test]
async fn get_model_info_matches_namespaced_suffix() {
    let darwin_code_home = tempdir().expect("temp dir");
    let config = ModelsManagerConfig::default();
    let mut remote = remote_model("gpt-image", "Image", /*priority*/ 0);
    remote.supports_image_detail_original = true;
    let manager = ModelsManager::new(
        darwin_code_home.path().to_path_buf(),
        Some(ModelsResponse {
            models: vec![remote],
        }),
        CollaborationModesConfig::default(),
    );
    let namespaced_model = "custom/gpt-image".to_string();

    let model_info = manager.get_model_info(&namespaced_model, &config).await;

    assert_eq!(model_info.slug, namespaced_model);
    assert!(model_info.supports_image_detail_original);
    assert!(!model_info.used_fallback_model_metadata);
}

#[tokio::test]
async fn get_model_info_rejects_multi_segment_namespace_suffix_matching() {
    let darwin_code_home = tempdir().expect("temp dir");
    let config = ModelsManagerConfig::default();
    let manager = ModelsManager::new(
        darwin_code_home.path().to_path_buf(),
        /*model_catalog*/ None,
        CollaborationModesConfig::default(),
    );
    let known_slug = manager
        .get_remote_models()
        .await
        .first()
        .expect("bundled models should include at least one model")
        .slug
        .clone();
    let namespaced_model = format!("ns1/ns2/{known_slug}");

    let model_info = manager.get_model_info(&namespaced_model, &config).await;

    assert_eq!(model_info.slug, namespaced_model);
    assert!(model_info.used_fallback_model_metadata);
}

#[tokio::test]
async fn refresh_available_models_sorts_by_priority() {
    let server = MockServer::start().await;
    let remote_models = vec![
        remote_model("priority-low", "Low", /*priority*/ 1),
        remote_model("priority-high", "High", /*priority*/ 0),
    ];
    let models_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: remote_models.clone(),
        },
    )
    .await;

    let darwin_code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("refresh succeeds");
    let cached_remote = manager.get_remote_models().await;
    assert_models_contain(&cached_remote, &remote_models);

    let available = manager.list_models(RefreshStrategy::OnlineIfUncached).await;
    let high_idx = available
        .iter()
        .position(|model| model.model == "priority-high")
        .expect("priority-high should be listed");
    let low_idx = available
        .iter()
        .position(|model| model.model == "priority-low")
        .expect("priority-low should be listed");
    assert!(
        high_idx < low_idx,
        "higher priority should be listed before lower priority"
    );
    assert_eq!(
        models_mock.requests().len(),
        1,
        "expected a single /models request"
    );
}

#[tokio::test]
async fn refresh_available_models_uses_provider_auth_token() {
    const ENV_KEY: &str = "DARWIN_CODE_TEST_MODELS_MANAGER_PROVIDER_TOKEN";
    unsafe {
        std::env::set_var(ENV_KEY, "provider-token");
    }

    let server = MockServer::start().await;
    let remote_models = vec![remote_model(
        "provider-model",
        "Provider",
        /*priority*/ 0,
    )];

    Mock::given(method("GET"))
        .and(path("/models"))
        .and(header_regex("Authorization", "Bearer provider-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(ModelsResponse {
                    models: remote_models.clone(),
                }),
        )
        .expect(1)
        .mount(&server)
        .await;

    let darwin_code_home = tempdir().expect("temp dir");
    let mut provider = ModelProviderInfo::create_openai_compatible_provider(
        "mock".to_string(),
        server.uri(),
        WireApi::Responses,
        ENV_KEY.to_string(),
    );
    provider.request_max_retries = Some(0);
    provider.stream_max_retries = Some(0);
    provider.stream_idle_timeout_ms = Some(5_000);
    let manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);

    manager
        .refresh_available_models(RefreshStrategy::Online)
        .await
        .expect("refresh succeeds");

    assert_models_contain(&manager.get_remote_models().await, &remote_models);
    unsafe {
        std::env::remove_var(ENV_KEY);
    }
}

#[tokio::test]
async fn refresh_available_models_uses_cache_when_fresh() {
    let server = MockServer::start().await;
    let remote_models = vec![remote_model("cached", "Cached", /*priority*/ 5)];
    let models_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: remote_models.clone(),
        },
    )
    .await;

    let darwin_code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("first refresh succeeds");
    assert_models_contain(&manager.get_remote_models().await, &remote_models);

    // Second call should read from cache and avoid the network.
    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("cached refresh succeeds");
    assert_models_contain(&manager.get_remote_models().await, &remote_models);
    assert_eq!(
        models_mock.requests().len(),
        1,
        "cache hit should avoid a second /models request"
    );
}

#[tokio::test]
async fn refresh_available_models_refetches_when_cache_stale() {
    let server = MockServer::start().await;
    let initial_models = vec![remote_model("stale", "Stale", /*priority*/ 1)];
    let initial_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: initial_models.clone(),
        },
    )
    .await;

    let darwin_code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("initial refresh succeeds");

    // Rewrite cache with an old timestamp so it is treated as stale.
    manager
        .cache_manager
        .manipulate_cache_for_test(|fetched_at| {
            *fetched_at = Utc::now() - chrono::Duration::hours(1);
        })
        .await
        .expect("cache manipulation succeeds");

    let updated_models = vec![remote_model("fresh", "Fresh", /*priority*/ 9)];
    server.reset().await;
    let refreshed_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: updated_models.clone(),
        },
    )
    .await;

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("second refresh succeeds");
    assert_models_contain(&manager.get_remote_models().await, &updated_models);
    assert_eq!(
        initial_mock.requests().len(),
        1,
        "initial refresh should only hit /models once"
    );
    assert_eq!(
        refreshed_mock.requests().len(),
        1,
        "stale cache refresh should fetch /models once"
    );
}

#[tokio::test]
async fn refresh_available_models_refetches_when_version_mismatch() {
    let server = MockServer::start().await;
    let initial_models = vec![remote_model("old", "Old", /*priority*/ 1)];
    let initial_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: initial_models.clone(),
        },
    )
    .await;

    let darwin_code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("initial refresh succeeds");

    manager
        .cache_manager
        .mutate_cache_for_test(|cache| {
            let client_version = crate::client_version_to_whole();
            cache.client_version = Some(format!("{client_version}-mismatch"));
        })
        .await
        .expect("cache mutation succeeds");

    let updated_models = vec![remote_model("new", "New", /*priority*/ 2)];
    server.reset().await;
    let refreshed_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: updated_models.clone(),
        },
    )
    .await;

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("second refresh succeeds");
    assert_models_contain(&manager.get_remote_models().await, &updated_models);
    assert_eq!(
        initial_mock.requests().len(),
        1,
        "initial refresh should only hit /models once"
    );
    assert_eq!(
        refreshed_mock.requests().len(),
        1,
        "version mismatch should fetch /models once"
    );
}

#[tokio::test]
async fn refresh_available_models_drops_removed_remote_models() {
    let server = MockServer::start().await;
    let initial_models = vec![remote_model(
        "remote-old",
        "Remote Old",
        /*priority*/ 1,
    )];
    let initial_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: initial_models,
        },
    )
    .await;

    let darwin_code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let mut manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);
    manager.cache_manager.set_ttl(Duration::ZERO);

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("initial refresh succeeds");

    server.reset().await;
    let refreshed_models = vec![remote_model(
        "remote-new",
        "Remote New",
        /*priority*/ 1,
    )];
    let refreshed_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: refreshed_models,
        },
    )
    .await;

    manager
        .refresh_available_models(RefreshStrategy::OnlineIfUncached)
        .await
        .expect("second refresh succeeds");

    let available = manager
        .try_list_models()
        .expect("models should be available");
    assert!(
        available.iter().any(|preset| preset.model == "remote-new"),
        "new remote model should be listed"
    );
    assert!(
        !available.iter().any(|preset| preset.model == "remote-old"),
        "removed remote model should not be listed"
    );
    assert_eq!(
        initial_mock.requests().len(),
        1,
        "initial refresh should only hit /models once"
    );
    assert_eq!(
        refreshed_mock.requests().len(),
        1,
        "second refresh should only hit /models once"
    );
}

#[tokio::test]
async fn refresh_available_models_fetches_standard_provider_models() {
    let server = MockServer::start().await;
    let dynamic_slug = "dynamic-model-only-for-test-byok";
    let models_mock = mount_models_once(
        &server,
        ModelsResponse {
            models: vec![remote_model(dynamic_slug, "BYOK", /*priority*/ 1)],
        },
    )
    .await;

    let darwin_code_home = tempdir().expect("temp dir");
    let provider = provider_for(server.uri());
    let manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);

    manager
        .refresh_available_models(RefreshStrategy::Online)
        .await
        .expect("refresh succeeds with BYOK provider");
    let cached_remote = manager.get_remote_models().await;
    assert!(
        cached_remote
            .iter()
            .any(|candidate| candidate.slug == dynamic_slug),
        "BYOK providers may refresh via standard /models"
    );
    assert_eq!(
        models_mock.requests().len(),
        1,
        "standard provider should fetch /models once"
    );
}

#[test]
fn build_available_models_picks_default_after_hiding_hidden_models() {
    let darwin_code_home = tempdir().expect("temp dir");
    let provider = provider_for("http://example.test".to_string());
    let manager =
        ModelsManager::with_provider_for_tests(darwin_code_home.path().to_path_buf(), provider);

    let hidden_model =
        remote_model_with_visibility("hidden", "Hidden", /*priority*/ 0, "hide");
    let visible_model =
        remote_model_with_visibility("visible", "Visible", /*priority*/ 1, "list");

    let expected_hidden = ModelPreset::from(hidden_model.clone());
    let mut expected_visible = ModelPreset::from(visible_model.clone());
    expected_visible.is_default = true;

    let available = manager.build_available_models(vec![hidden_model, visible_model]);

    assert_eq!(available, vec![expected_hidden, expected_visible]);
}

#[test]
fn bundled_models_json_roundtrips() {
    let response = crate::bundled_models_response()
        .unwrap_or_else(|err| panic!("bundled models.json should parse: {err}"));

    let serialized =
        serde_json::to_string(&response).expect("bundled models.json should serialize");
    let roundtripped: ModelsResponse =
        serde_json::from_str(&serialized).expect("serialized models.json should deserialize");

    assert_eq!(
        response, roundtripped,
        "bundled models.json should round trip through serde"
    );
    assert!(
        !response.models.is_empty(),
        "bundled models.json should contain at least one model"
    );
}
