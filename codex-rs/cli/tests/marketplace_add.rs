use anyhow::Result;
use darwin_code_config::CONFIG_TOML_FILE;
use darwin_code_core::plugins::marketplace_install_root;
use predicates::str::contains;
use pretty_assertions::assert_eq;
use std::path::Path;
use tempfile::TempDir;

fn darwin_code_command(darwin_code_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(darwin_code_utils_cargo_bin::cargo_bin("darwin-code")?);
    cmd.env("DARWIN_CODE_HOME", darwin_code_home);
    Ok(cmd)
}

fn write_marketplace_source(source: &Path, marker: &str) -> Result<()> {
    std::fs::create_dir_all(source.join(".agents/plugins"))?;
    std::fs::create_dir_all(source.join("plugins/sample/.darwin-code-plugin"))?;
    std::fs::write(
        source.join(".agents/plugins/marketplace.json"),
        r#"{
  "name": "debug",
  "plugins": [
    {
      "name": "sample",
      "source": {
        "source": "local",
        "path": "./plugins/sample"
      }
    }
  ]
}"#,
    )?;
    std::fs::write(
        source.join("plugins/sample/.darwin-code-plugin/plugin.json"),
        r#"{"name":"sample"}"#,
    )?;
    std::fs::write(source.join("plugins/sample/marker.txt"), marker)?;
    Ok(())
}

#[tokio::test]
async fn marketplace_add_local_directory_source() -> Result<()> {
    let darwin_code_home = TempDir::new()?;
    let source = TempDir::new()?;
    write_marketplace_source(source.path(), "local ref")?;
    let source_parent = source.path().parent().unwrap();
    let source_arg = format!("./{}", source.path().file_name().unwrap().to_string_lossy());

    darwin_code_command(darwin_code_home.path())?
        .current_dir(source_parent)
        .args(["plugin", "marketplace", "add", source_arg.as_str()])
        .assert()
        .success();

    let installed_root = marketplace_install_root(darwin_code_home.path()).join("debug");
    assert!(!installed_root.exists());

    let config = std::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE))?;
    let config: toml::Value = toml::from_str(&config)?;
    let expected_source = source.path().canonicalize()?.display().to_string();
    assert_eq!(
        config["marketplaces"]["debug"]["source_type"].as_str(),
        Some("local")
    );
    assert_eq!(
        config["marketplaces"]["debug"]["source"].as_str(),
        Some(expected_source.as_str())
    );

    Ok(())
}

#[tokio::test]
async fn marketplace_add_rejects_local_manifest_file_source() -> Result<()> {
    let darwin_code_home = TempDir::new()?;
    let source = TempDir::new()?;
    write_marketplace_source(source.path(), "local ref")?;
    let manifest_path = source.path().join(".agents/plugins/marketplace.json");

    darwin_code_command(darwin_code_home.path())?
        .args([
            "plugin",
            "marketplace",
            "add",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains(
            "local marketplace source must be a directory, not a file",
        ));

    Ok(())
}

#[tokio::test]
async fn marketplace_add_rejects_sparse_for_local_directory_source() -> Result<()> {
    let darwin_code_home = TempDir::new()?;
    let source = TempDir::new()?;
    write_marketplace_source(source.path(), "local ref")?;

    darwin_code_command(darwin_code_home.path())?
        .args([
            "plugin",
            "marketplace",
            "add",
            "--sparse",
            ".agents",
            source.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains(
            "--sparse is only supported for git marketplace sources",
        ));

    Ok(())
}
