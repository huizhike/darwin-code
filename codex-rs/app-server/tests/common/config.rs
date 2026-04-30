use darwin_code_features::FEATURES;
use darwin_code_features::Feature;
use std::collections::BTreeMap;
use std::path::Path;

pub const DEFAULT_BYOK_TEST_PROVIDER_CONFIG: &str = r#"
[providers.openai]
family = "openai-compatible"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
api_key = "test-direct-api-key"
"#;

pub fn ensure_default_byok_provider_config(darwin_code_home: &Path) -> std::io::Result<()> {
    let config_path = darwin_code_home.join("config.toml");
    let mut contents = std::fs::read_to_string(&config_path).unwrap_or_default();
    if contents.contains("[providers.") || contents.contains("[openai_compatible_provider]") {
        return Ok(());
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(DEFAULT_BYOK_TEST_PROVIDER_CONFIG);
    std::fs::write(config_path, contents)
}

pub fn write_mock_responses_config_toml(
    darwin_code_home: &Path,
    server_uri: &str,
    feature_flags: &BTreeMap<Feature, bool>,
    auto_compact_limit: i64,
    use_openai_provider_name_for_remote_compaction: Option<bool>,
    model_provider_id: &str,
    compact_prompt: &str,
) -> std::io::Result<()> {
    // Phase 1: build the features block for config.toml.
    let mut features = BTreeMap::new();
    for (feature, enabled) in feature_flags {
        features.insert(*feature, *enabled);
    }
    let feature_entries = features
        .into_iter()
        .map(|(feature, enabled)| {
            let key = FEATURES
                .iter()
                .find(|spec| spec.id == feature)
                .map(|spec| spec.key)
                .unwrap_or_else(|| panic!("missing feature key for {feature:?}"));
            format!("{key} = {enabled}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Phase 2: build provider-specific config bits.
    let provider_name = if use_openai_provider_name_for_remote_compaction.unwrap_or(false) {
        "OpenAI"
    } else {
        "Mock provider for test"
    };
    let provider_block = format!(
        r#"
[providers.{model_provider_id}]
family = "openai-compatible"
name = "{provider_name}"
base_url = "{server_uri}/v1"
api_key = "test-direct-api-key"
"#
    );
    // Phase 3: write the final config file.
    let config_toml = darwin_code_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"
compact_prompt = "{compact_prompt}"
model_auto_compact_token_limit = {auto_compact_limit}

model_provider = "{model_provider_id}"

[features]
{feature_entries}
{provider_block}
"#
        ),
    )
}

pub fn write_mock_responses_config_toml_with_base_url(
    darwin_code_home: &Path,
    server_uri: &str,
    _base_url: &str,
) -> std::io::Result<()> {
    let config_toml = darwin_code_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[providers.mock_provider]
family = "openai-compatible"
name = "Mock provider for test"
base_url = "{server_uri}/v1"
api_key = "test-direct-api-key"
"#
        ),
    )
}
