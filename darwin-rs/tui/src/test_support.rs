pub(crate) use darwin_code_utils_absolute_path::test_support::PathBufExt;
pub(crate) use darwin_code_utils_absolute_path::test_support::test_path_buf;
use std::path::Path;

pub(crate) const DEFAULT_BYOK_TEST_PROVIDER_CONFIG: &str = r#"
[providers.openai]
family = "openai-compatible"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
api_key = "test-direct-api-key"
"#;

pub(crate) fn ensure_default_byok_provider_config(darwin_code_home: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(darwin_code_home)?;
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

pub(crate) fn test_path_display(path: &str) -> String {
    test_path_buf(path).display().to_string()
}
