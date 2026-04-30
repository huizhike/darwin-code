use anyhow::Result;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

fn darwin_code_command(darwin_code_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(darwin_code_utils_cargo_bin::cargo_bin("darwin_code")?);
    cmd.env("DARWIN_CODE_HOME", darwin_code_home);
    Ok(cmd)
}

fn write_byok_test_config(darwin_code_home: &Path) -> Result<()> {
    std::fs::write(
        darwin_code_home.join("config.toml"),
        r#"
[providers.openai]
family = "openai-compatible"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
api_key = "test-direct-api-key"
"#,
    )?;
    Ok(())
}

#[tokio::test]
async fn marketplace_upgrade_runs_under_plugin() -> Result<()> {
    let darwin_code_home = TempDir::new()?;
    write_byok_test_config(darwin_code_home.path())?;

    darwin_code_command(darwin_code_home.path())?
        .args(["plugin", "marketplace", "upgrade"])
        .assert()
        .success()
        .stdout(contains("No configured Git marketplaces to upgrade."));

    Ok(())
}

#[tokio::test]
async fn marketplace_upgrade_no_longer_runs_at_top_level() -> Result<()> {
    let darwin_code_home = TempDir::new()?;

    darwin_code_command(darwin_code_home.path())?
        .args(["marketplace", "upgrade"])
        .assert()
        .failure()
        .stderr(contains("unexpected argument 'upgrade' found"));

    Ok(())
}
