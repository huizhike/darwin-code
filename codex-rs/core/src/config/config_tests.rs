use crate::agents_md::DEFAULT_AGENTS_MD_FILENAME;
use crate::agents_md::LOCAL_AGENTS_MD_FILENAME;
use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::config::edit::apply_blocking;
use crate::config_loader::RequirementSource;
use crate::config_loader::project_trust_key;
use crate::plugins::PluginsManager;
use assert_matches::assert_matches;
use darwin_code_config::CONFIG_TOML_FILE;
use darwin_code_config::config_toml::AgentRoleToml;
use darwin_code_config::config_toml::AgentsToml;
use darwin_code_config::config_toml::ConfigToml;
use darwin_code_config::config_toml::ProjectConfig;
use darwin_code_config::config_toml::RealtimeAudioConfig;
use darwin_code_config::config_toml::RealtimeConfig;
use darwin_code_config::config_toml::ToolsToml;
use darwin_code_config::permissions_toml::FilesystemPermissionToml;
use darwin_code_config::permissions_toml::FilesystemPermissionsToml;
use darwin_code_config::permissions_toml::NetworkDomainPermissionToml;
use darwin_code_config::permissions_toml::NetworkDomainPermissionsToml;
use darwin_code_config::permissions_toml::NetworkToml;
use darwin_code_config::permissions_toml::PermissionProfileToml;
use darwin_code_config::permissions_toml::PermissionsToml;
use darwin_code_config::profile_toml::ConfigProfile;
use darwin_code_config::types::AppToolApproval;
use darwin_code_config::types::ApprovalsReviewer;
use darwin_code_config::types::BundledSkillsConfig;
use darwin_code_config::types::HistoryPersistence;
use darwin_code_config::types::McpServerToolConfig;
use darwin_code_config::types::McpServerTransportConfig;
use darwin_code_config::types::MemoriesConfig;
use darwin_code_config::types::MemoriesToml;
use darwin_code_config::types::ModelAvailabilityNuxConfig;
use darwin_code_config::types::NotificationCondition;
use darwin_code_config::types::NotificationMethod;
use darwin_code_config::types::Notifications;
use darwin_code_config::types::OtelExporterKind;
use darwin_code_config::types::SandboxWorkspaceWrite;
use darwin_code_config::types::SkillsConfig;
use darwin_code_config::types::ToolSuggestDiscoverableType;
use darwin_code_config::types::Tui;
use darwin_code_config::types::TuiNotificationSettings;
use darwin_code_exec_server::LOCAL_FS;
use darwin_code_features::Feature;
use darwin_code_features::FeaturesToml;
use darwin_code_model_provider_info::OPENAI_PROVIDER_ID;
use darwin_code_model_provider_info::WireApi;
use darwin_code_models_manager::bundled_models_response;
use darwin_code_protocol::permissions::FileSystemAccessMode;
use darwin_code_protocol::permissions::FileSystemPath;
use darwin_code_protocol::permissions::FileSystemSandboxEntry;
use darwin_code_protocol::permissions::FileSystemSandboxPolicy;
use darwin_code_protocol::permissions::FileSystemSpecialPath;
use darwin_code_protocol::permissions::NetworkSandboxPolicy;
use darwin_code_protocol::protocol::ReadOnlyAccess;
use serde::Deserialize;
use tempfile::tempdir;

use super::*;
use core_test_support::PathBufExt;
use core_test_support::PathExt;
use core_test_support::TempDirExt;
use core_test_support::test_absolute_path;
use pretty_assertions::assert_eq;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

fn stdio_mcp(command: &str) -> McpServerConfig {
    McpServerConfig {
        transport: McpServerTransportConfig::Stdio {
            command: command.to_string(),
            args: Vec::new(),
            env: None,
            env_vars: Vec::new(),
            cwd: None,
        },
        experimental_environment: None,
        enabled: true,
        required: false,
        supports_parallel_tool_calls: false,
        disabled_reason: None,
        startup_timeout_sec: None,
        tool_timeout_sec: None,
        default_tools_approval_mode: None,
        enabled_tools: None,
        disabled_tools: None,
        tools: HashMap::new(),
    }
}

fn http_mcp(url: &str) -> McpServerConfig {
    McpServerConfig {
        transport: McpServerTransportConfig::StreamableHttp {
            url: url.to_string(),
            bearer_token_env_var: None,
            http_headers: None,
            env_http_headers: None,
        },
        experimental_environment: None,
        enabled: true,
        required: false,
        supports_parallel_tool_calls: false,
        disabled_reason: None,
        startup_timeout_sec: None,
        tool_timeout_sec: None,
        default_tools_approval_mode: None,
        enabled_tools: None,
        disabled_tools: None,
        tools: HashMap::new(),
    }
}

async fn load_workspace_permission_profile(
    profile: PermissionProfileToml,
) -> std::io::Result<Config> {
    let darwin_code_home = TempDir::new()?;
    let cwd = TempDir::new()?;
    std::fs::write(cwd.path().join(".git"), "gitdir: nowhere")?;

    Config::load_from_base_config_with_overrides(
        ConfigToml {
            default_permissions: Some("workspace".to_string()),
            permissions: Some(PermissionsToml {
                entries: BTreeMap::from([("workspace".to_string(), profile)]),
            }),
            ..Default::default()
        },
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await
}

#[tokio::test]
async fn set_model_updates_defaults() -> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .set_model(Some("gpt-5.1-darwin-code"), Some(ReasoningEffort::High))
        .apply()
        .await?;

    let serialized =
        tokio::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE)).await?;
    let parsed: ConfigToml = toml::from_str(&serialized)?;

    assert_eq!(parsed.model.as_deref(), Some("gpt-5.1-darwin-code"));
    assert_eq!(parsed.model_reasoning_effort, Some(ReasoningEffort::High));

    Ok(())
}

#[tokio::test]
async fn set_model_overwrites_existing_model() -> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let config_path = darwin_code_home.path().join(CONFIG_TOML_FILE);

    tokio::fs::write(
        &config_path,
        r#"
model = "gpt-5.1-darwin-code"
model_reasoning_effort = "medium"

[profiles.dev]
model = "gpt-4.1"
"#,
    )
    .await?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .set_model(Some("o4-mini"), Some(ReasoningEffort::High))
        .apply()
        .await?;

    let serialized = tokio::fs::read_to_string(config_path).await?;
    let parsed: ConfigToml = toml::from_str(&serialized)?;

    assert_eq!(parsed.model.as_deref(), Some("o4-mini"));
    assert_eq!(parsed.model_reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(
        parsed
            .profiles
            .get("dev")
            .and_then(|profile| profile.model.as_deref()),
        Some("gpt-4.1"),
    );

    Ok(())
}

#[tokio::test]
async fn set_model_updates_profile() -> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .with_profile(Some("dev"))
        .set_model(Some("gpt-5.1-darwin-code"), Some(ReasoningEffort::Medium))
        .apply()
        .await?;

    let serialized =
        tokio::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE)).await?;
    let parsed: ConfigToml = toml::from_str(&serialized)?;
    let profile = parsed
        .profiles
        .get("dev")
        .expect("profile should be created");

    assert_eq!(profile.model.as_deref(), Some("gpt-5.1-darwin-code"));
    assert_eq!(
        profile.model_reasoning_effort,
        Some(ReasoningEffort::Medium)
    );

    Ok(())
}

#[tokio::test]
async fn set_model_updates_existing_profile() -> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let config_path = darwin_code_home.path().join(CONFIG_TOML_FILE);

    tokio::fs::write(
        &config_path,
        r#"
[profiles.dev]
model = "gpt-4"
model_reasoning_effort = "medium"

[profiles.prod]
model = "gpt-5.1-darwin-code"
"#,
    )
    .await?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .with_profile(Some("dev"))
        .set_model(Some("o4-high"), Some(ReasoningEffort::Medium))
        .apply()
        .await?;

    let serialized = tokio::fs::read_to_string(config_path).await?;
    let parsed: ConfigToml = toml::from_str(&serialized)?;

    let dev_profile = parsed
        .profiles
        .get("dev")
        .expect("dev profile should survive updates");
    assert_eq!(dev_profile.model.as_deref(), Some("o4-high"));
    assert_eq!(
        dev_profile.model_reasoning_effort,
        Some(ReasoningEffort::Medium)
    );

    assert_eq!(
        parsed
            .profiles
            .get("prod")
            .and_then(|profile| profile.model.as_deref()),
        Some("gpt-5.1-darwin-code"),
    );

    Ok(())
}

#[tokio::test]
async fn set_feature_enabled_updates_profile() -> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .with_profile(Some("dev"))
        .set_feature_enabled("guardian_approval", /*enabled*/ true)
        .apply()
        .await?;

    let serialized =
        tokio::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE)).await?;
    let parsed: ConfigToml = toml::from_str(&serialized)?;
    let profile = parsed
        .profiles
        .get("dev")
        .expect("profile should be created");

    assert_eq!(
        profile
            .features
            .as_ref()
            .and_then(|features| features.entries().get("guardian_approval").copied()),
        Some(true),
    );
    assert_eq!(
        parsed
            .features
            .as_ref()
            .and_then(|features| features.entries().get("guardian_approval").copied()),
        None,
    );

    Ok(())
}

#[tokio::test]
async fn set_feature_enabled_persists_default_false_feature_disable_in_profile()
-> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .with_profile(Some("dev"))
        .set_feature_enabled("guardian_approval", /*enabled*/ true)
        .apply()
        .await?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .with_profile(Some("dev"))
        .set_feature_enabled("guardian_approval", /*enabled*/ false)
        .apply()
        .await?;

    let serialized =
        tokio::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE)).await?;
    let parsed: ConfigToml = toml::from_str(&serialized)?;
    let profile = parsed
        .profiles
        .get("dev")
        .expect("profile should be created");

    assert_eq!(
        profile
            .features
            .as_ref()
            .and_then(|features| features.entries().get("guardian_approval").copied()),
        Some(false),
    );
    assert_eq!(
        parsed
            .features
            .as_ref()
            .and_then(|features| features.entries().get("guardian_approval").copied()),
        None,
    );

    Ok(())
}

#[tokio::test]
async fn set_feature_enabled_profile_disable_overrides_root_enable() -> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .set_feature_enabled("guardian_approval", /*enabled*/ true)
        .apply()
        .await?;

    ConfigEditsBuilder::new(darwin_code_home.path())
        .with_profile(Some("dev"))
        .set_feature_enabled("guardian_approval", /*enabled*/ false)
        .apply()
        .await?;

    let serialized =
        tokio::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE)).await?;
    let parsed: ConfigToml = toml::from_str(&serialized)?;
    let profile = parsed
        .profiles
        .get("dev")
        .expect("profile should be created");

    assert_eq!(
        parsed
            .features
            .as_ref()
            .and_then(|features| features.entries().get("guardian_approval").copied()),
        Some(true),
    );
    assert_eq!(
        profile
            .features
            .as_ref()
            .and_then(|features| features.entries().get("guardian_approval").copied()),
        Some(false),
    );

    Ok(())
}

struct PrecedenceTestFixture {
    cwd: TempDir,
    darwin_code_home: TempDir,
    cfg: ConfigToml,
    model_provider_map: HashMap<String, ModelProviderInfo>,
    openai_provider: ModelProviderInfo,
    openai_custom_provider: ModelProviderInfo,
}

impl PrecedenceTestFixture {
    fn cwd(&self) -> AbsolutePathBuf {
        self.cwd.abs()
    }

    fn cwd_path(&self) -> PathBuf {
        self.cwd.path().to_path_buf()
    }

    fn darwin_code_home(&self) -> AbsolutePathBuf {
        self.darwin_code_home.abs()
    }
}

#[tokio::test]
async fn cli_override_sets_compact_prompt() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let overrides = ConfigOverrides {
        compact_prompt: Some("Use the compact override".to_string()),
        ..Default::default()
    };

    let config = Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        overrides,
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(
        config.compact_prompt.as_deref(),
        Some("Use the compact override")
    );

    Ok(())
}

#[test]
fn external_and_removed_config_surfaces_do_not_block_config_toml_load() {
    fn key(parts: &[&str]) -> String {
        parts.concat()
    }

    fn table(parts: &[&str], body: &str) -> String {
        format!("[{}]\n{body}\n", key(parts))
    }

    fn scalar(parts: &[&str], value: &str) -> String {
        format!("{} = {value}\n", key(parts))
    }

    // Darwin Code shares ~/.codex/config.toml with upstream Codex/OMX installs.
    // Runtime config parsing must therefore tolerate foreign or legacy sections
    // such as [env] and [notice] instead of failing before BYOK provider config
    // can be loaded. Unsupported Darwin product surfaces remain removed from the
    // typed ConfigToml model; serde ignores them while the generated schema stays
    // strict via schemars(deny_unknown_fields).
    let cases = vec![
        (
            key(&["en", "v"]),
            table(&["en", "v"], "USE_OMX_EXPLORE_CMD = \"1\""),
        ),
        (
            key(&["not", "ice"]),
            table(&["not", "ice"], "hide_full_access_warning = true"),
        ),
        (
            key(&["ana", "lytics"]),
            table(&["ana", "lytics"], "enabled = true"),
        ),
        (
            key(&["feed", "back"]),
            table(&["feed", "back"], "enabled = true"),
        ),
        (
            key(&["app", "_launcher"]),
            table(&["app", "_launcher"], "enabled = true"),
        ),
        (key(&["ot", "el"]), table(&["ot", "el"], "enabled = true")),
        (
            key(&["au", "dio"]),
            table(&["au", "dio"], "microphone = \"USB Mic\""),
        ),
        (
            key(&["real", "time"]),
            table(&["real", "time"], "voice = \"marin\""),
        ),
        (
            key(&["file", "_opener"]),
            scalar(&["file", "_opener"], "\"vscode\""),
        ),
        (
            key(&["experimental_", "real", "time_start_instructions"]),
            scalar(
                &["experimental_", "real", "time_start_instructions"],
                "\"start\"",
            ),
        ),
        (
            key(&["experimental_", "real", "time_ws_base_url"]),
            scalar(
                &["experimental_", "real", "time_ws_base_url"],
                "\"http://127.0.0.1:8011\"",
            ),
        ),
        (
            key(&["experimental_", "real", "time_ws_backend_prompt"]),
            scalar(
                &["experimental_", "real", "time_ws_backend_prompt"],
                "\"prompt\"",
            ),
        ),
        (
            key(&["experimental_", "real", "time_ws_startup_context"]),
            scalar(
                &["experimental_", "real", "time_ws_startup_context"],
                "\"ctx\"",
            ),
        ),
        (
            key(&["experimental_", "real", "time_ws_model"]),
            scalar(
                &["experimental_", "real", "time_ws_model"],
                "\"realtime-test-model\"",
            ),
        ),
        (
            key(&["experimental_", "instructions_file"]),
            scalar(
                &["experimental_", "instructions_file"],
                "\"/tmp/instructions.md\"",
            ),
        ),
        (
            key(&["experimental_", "compact_prompt_file"]),
            scalar(
                &["experimental_", "compact_prompt_file"],
                "\"/tmp/compact.md\"",
            ),
        ),
        (
            key(&["experimental_", "use_unified_exec_tool"]),
            scalar(&["experimental_", "use_unified_exec_tool"], "true"),
        ),
        (
            key(&["experimental_", "use_freeform_apply_patch"]),
            scalar(&["experimental_", "use_freeform_apply_patch"], "true"),
        ),
    ];

    for (name, snippet) in cases {
        toml::from_str::<ConfigToml>(&snippet)
            .unwrap_or_else(|err| panic!("{name} should not block runtime config load: {err}"));
    }
}

#[tokio::test]
async fn load_config_uses_requirements_guardian_policy_config() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let config_layer_stack = ConfigLayerStack::new(
        Vec::new(),
        Default::default(),
        crate::config_loader::ConfigRequirementsToml {
            guardian_policy_config: Some(
                "  Use the workspace-managed guardian policy.  ".to_string(),
            ),
            ..Default::default()
        },
    )
    .map_err(std::io::Error::other)?;

    let mut cfg = ConfigToml::default();
    ensure_default_byok_provider_config_toml_for_tests(&mut cfg);
    let config = Config::load_config_with_layer_stack(
        LOCAL_FS.as_ref(),
        cfg,
        ConfigOverrides {
            cwd: Some(darwin_code_home.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
        config_layer_stack,
    )
    .await?;

    assert_eq!(
        config.guardian_policy_config.as_deref(),
        Some("Use the workspace-managed guardian policy.")
    );

    Ok(())
}

#[tokio::test]
async fn load_config_ignores_empty_requirements_guardian_policy_config() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let config_layer_stack = ConfigLayerStack::new(
        Vec::new(),
        Default::default(),
        crate::config_loader::ConfigRequirementsToml {
            guardian_policy_config: Some("   ".to_string()),
            ..Default::default()
        },
    )
    .map_err(std::io::Error::other)?;

    let mut cfg = ConfigToml::default();
    ensure_default_byok_provider_config_toml_for_tests(&mut cfg);
    let config = Config::load_config_with_layer_stack(
        LOCAL_FS.as_ref(),
        cfg,
        ConfigOverrides {
            cwd: Some(darwin_code_home.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
        config_layer_stack,
    )
    .await?;

    assert_eq!(config.guardian_policy_config, None);

    Ok(())
}

#[tokio::test]
async fn load_config_rejects_missing_agent_role_config_file() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let missing_path = darwin_code_home
        .path()
        .join("agents")
        .join("researcher.toml");
    let cfg = ConfigToml {
        agents: Some(AgentsToml {
            max_threads: None,
            max_depth: None,
            job_max_runtime_seconds: None,
            roles: BTreeMap::from([(
                "researcher".to_string(),
                AgentRoleToml {
                    description: Some("Research role".to_string()),
                    config_file: Some(missing_path.abs()),
                    nickname_candidates: None,
                },
            )]),
        }),
        ..Default::default()
    };

    let result = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await;
    let err = result.expect_err("missing role config file should be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    let message = err.to_string();
    assert!(message.contains("agents.researcher.config_file"));
    assert!(message.contains("must point to an existing file"));

    Ok(())
}

#[tokio::test]
async fn agent_role_relative_config_file_resolves_against_config_toml() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let role_config_path = darwin_code_home
        .path()
        .join("agents")
        .join("researcher.toml");
    tokio::fs::create_dir_all(
        role_config_path
            .parent()
            .expect("role config should have a parent directory"),
    )
    .await?;
    tokio::fs::write(
        &role_config_path,
        "developer_instructions = \"Research carefully\"\nmodel = \"gpt-5\"",
    )
    .await?;
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[agents.researcher]
description = "Research role"
config_file = "./agents/researcher.toml"
nickname_candidates = ["Hypatia", "Noether"]
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.config_file.as_ref()),
        Some(&role_config_path)
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Hypatia", "Noether"])
    );

    Ok(())
}

#[tokio::test]
async fn agent_role_file_metadata_overrides_config_toml_metadata() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let role_config_path = darwin_code_home
        .path()
        .join("agents")
        .join("researcher.toml");
    tokio::fs::create_dir_all(
        role_config_path
            .parent()
            .expect("role config should have a parent directory"),
    )
    .await?;
    tokio::fs::write(
        &role_config_path,
        r#"
description = "Role metadata from file"
nickname_candidates = ["Hypatia"]
developer_instructions = "Research carefully"
model = "gpt-5"
"#,
    )
    .await?;
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[agents.researcher]
description = "Research role from config"
config_file = "./agents/researcher.toml"
nickname_candidates = ["Noether"]
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;
    let role = config
        .agent_roles
        .get("researcher")
        .expect("researcher role should load");
    assert_eq!(role.description.as_deref(), Some("Role metadata from file"));
    assert_eq!(role.config_file.as_ref(), Some(&role_config_path));
    assert_eq!(
        role.nickname_candidates
            .as_ref()
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Hypatia"])
    );

    Ok(())
}

#[tokio::test]
async fn agent_role_file_without_developer_instructions_is_dropped_with_warning()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let repo_root = TempDir::new()?;
    let nested_cwd = repo_root.path().join("packages").join("app");
    std::fs::create_dir_all(repo_root.path().join(".git"))?;
    std::fs::create_dir_all(&nested_cwd)?;

    let workspace_key = repo_root.path().to_string_lossy().replace('\\', "\\\\");
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        format!(
            r#"[projects."{workspace_key}"]
trust_level = "trusted"
"#
        ),
    )
    .await?;

    let standalone_agents_dir = repo_root.path().join(".darwin_code").join("agents");
    tokio::fs::create_dir_all(&standalone_agents_dir).await?;
    tokio::fs::write(
        standalone_agents_dir.join("researcher.toml"),
        r#"
name = "researcher"
description = "Role metadata from file"
model = "gpt-5"
"#,
    )
    .await?;
    tokio::fs::write(
        standalone_agents_dir.join("reviewer.toml"),
        r#"
name = "reviewer"
description = "Review role"
developer_instructions = "Review carefully"
model = "gpt-5"
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .harness_overrides(ConfigOverrides {
            cwd: Some(nested_cwd),
            ..Default::default()
        })
        .build()
        .await?;
    assert!(!config.agent_roles.contains_key("researcher"));
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.description.as_deref()),
        Some("Review role")
    );
    assert!(
        config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("must define `developer_instructions`"))
    );

    Ok(())
}

#[tokio::test]
async fn legacy_agent_role_config_file_allows_missing_developer_instructions() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    let role_config_path = darwin_code_home
        .path()
        .join("agents")
        .join("researcher.toml");
    tokio::fs::create_dir_all(
        role_config_path
            .parent()
            .expect("role config should have a parent directory"),
    )
    .await?;
    tokio::fs::write(
        &role_config_path,
        r#"
model = "gpt-5"
model_reasoning_effort = "high"
"#,
    )
    .await?;
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[agents.researcher]
description = "Research role from config"
config_file = "./agents/researcher.toml"
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.description.as_deref()),
        Some("Research role from config")
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.config_file.as_ref()),
        Some(&role_config_path)
    );

    Ok(())
}

#[tokio::test]
async fn agent_role_without_description_after_merge_is_dropped_with_warning() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    let role_config_path = darwin_code_home
        .path()
        .join("agents")
        .join("researcher.toml");
    tokio::fs::create_dir_all(
        role_config_path
            .parent()
            .expect("role config should have a parent directory"),
    )
    .await?;
    tokio::fs::write(
        &role_config_path,
        r#"
developer_instructions = "Research carefully"
model = "gpt-5"
"#,
    )
    .await?;
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[agents.researcher]
config_file = "./agents/researcher.toml"

[agents.reviewer]
description = "Review role"
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;
    assert!(!config.agent_roles.contains_key("researcher"));
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.description.as_deref()),
        Some("Review role")
    );
    assert!(
        config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("agent role `researcher` must define a description"))
    );

    Ok(())
}

#[tokio::test]
async fn discovered_agent_role_file_without_name_is_dropped_with_warning() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let repo_root = TempDir::new()?;
    let nested_cwd = repo_root.path().join("packages").join("app");
    std::fs::create_dir_all(repo_root.path().join(".git"))?;
    std::fs::create_dir_all(&nested_cwd)?;

    let workspace_key = repo_root.path().to_string_lossy().replace('\\', "\\\\");
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        format!(
            r#"[projects."{workspace_key}"]
trust_level = "trusted"
"#
        ),
    )
    .await?;

    let standalone_agents_dir = repo_root.path().join(".darwin_code").join("agents");
    tokio::fs::create_dir_all(&standalone_agents_dir).await?;
    tokio::fs::write(
        standalone_agents_dir.join("researcher.toml"),
        r#"
description = "Role metadata from file"
developer_instructions = "Research carefully"
"#,
    )
    .await?;
    tokio::fs::write(
        standalone_agents_dir.join("reviewer.toml"),
        r#"
name = "reviewer"
description = "Review role"
developer_instructions = "Review carefully"
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .harness_overrides(ConfigOverrides {
            cwd: Some(nested_cwd),
            ..Default::default()
        })
        .build()
        .await?;
    assert!(!config.agent_roles.contains_key("researcher"));
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.description.as_deref()),
        Some("Review role")
    );
    assert!(
        config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("must define a non-empty `name`"))
    );

    Ok(())
}

#[tokio::test]
async fn agent_role_file_name_takes_precedence_over_config_key() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let role_config_path = darwin_code_home
        .path()
        .join("agents")
        .join("researcher.toml");
    tokio::fs::create_dir_all(
        role_config_path
            .parent()
            .expect("role config should have a parent directory"),
    )
    .await?;
    tokio::fs::write(
        &role_config_path,
        r#"
name = "archivist"
description = "Role metadata from file"
developer_instructions = "Research carefully"
model = "gpt-5"
"#,
    )
    .await?;
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[agents.researcher]
description = "Research role from config"
config_file = "./agents/researcher.toml"
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;
    assert_eq!(config.agent_roles.contains_key("researcher"), false);
    let role = config
        .agent_roles
        .get("archivist")
        .expect("role should use file-provided name");
    assert_eq!(role.description.as_deref(), Some("Role metadata from file"));
    assert_eq!(role.config_file.as_ref(), Some(&role_config_path));

    Ok(())
}

#[tokio::test]
async fn loads_legacy_split_agent_roles_from_config_toml() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let researcher_path = darwin_code_home
        .path()
        .join("agents")
        .join("researcher.toml");
    let reviewer_path = darwin_code_home.path().join("agents").join("reviewer.toml");
    tokio::fs::create_dir_all(
        researcher_path
            .parent()
            .expect("role config should have a parent directory"),
    )
    .await?;
    tokio::fs::write(
        &researcher_path,
        "developer_instructions = \"Research carefully\"\nmodel = \"gpt-5\"",
    )
    .await?;
    tokio::fs::write(
        &reviewer_path,
        "developer_instructions = \"Review carefully\"\nmodel = \"gpt-4.1\"",
    )
    .await?;
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[agents.researcher]
description = "Research role"
config_file = "./agents/researcher.toml"
nickname_candidates = ["Hypatia", "Noether"]

[agents.reviewer]
description = "Review role"
config_file = "./agents/reviewer.toml"
nickname_candidates = ["Atlas"]
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.description.as_deref()),
        Some("Research role")
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.config_file.as_ref()),
        Some(&researcher_path)
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Hypatia", "Noether"])
    );
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.description.as_deref()),
        Some("Review role")
    );
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.config_file.as_ref()),
        Some(&reviewer_path)
    );
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Atlas"])
    );

    Ok(())
}

#[tokio::test]
async fn discovers_multiple_standalone_agent_role_files() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let repo_root = TempDir::new()?;
    let nested_cwd = repo_root.path().join("packages").join("app");
    std::fs::create_dir_all(repo_root.path().join(".git"))?;
    std::fs::create_dir_all(&nested_cwd)?;

    let workspace_key = repo_root.path().to_string_lossy().replace('\\', "\\\\");
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        format!(
            r#"[projects."{workspace_key}"]
trust_level = "trusted"
"#
        ),
    )?;

    let root_agent = repo_root
        .path()
        .join(".darwin_code")
        .join("agents")
        .join("root.toml");
    std::fs::create_dir_all(
        root_agent
            .parent()
            .expect("root agent should have a parent directory"),
    )?;
    std::fs::write(
        &root_agent,
        r#"
name = "researcher"
description = "from root"
developer_instructions = "Research carefully"
"#,
    )?;

    let nested_agent = repo_root
        .path()
        .join("packages")
        .join(".darwin_code")
        .join("agents")
        .join("review")
        .join("nested.toml");
    std::fs::create_dir_all(
        nested_agent
            .parent()
            .expect("nested agent should have a parent directory"),
    )?;
    std::fs::write(
        &nested_agent,
        r#"
name = "reviewer"
description = "from nested"
nickname_candidates = ["Atlas"]
developer_instructions = "Review carefully"
"#,
    )?;

    let sibling_agent = repo_root
        .path()
        .join("packages")
        .join(".darwin_code")
        .join("agents")
        .join("writer.toml");
    std::fs::create_dir_all(
        sibling_agent
            .parent()
            .expect("sibling agent should have a parent directory"),
    )?;
    std::fs::write(
        &sibling_agent,
        r#"
name = "writer"
description = "from sibling"
nickname_candidates = ["Sagan"]
developer_instructions = "Write carefully"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .harness_overrides(ConfigOverrides {
            cwd: Some(nested_cwd),
            ..Default::default()
        })
        .build()
        .await?;

    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.description.as_deref()),
        Some("from root")
    );
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.description.as_deref()),
        Some("from nested")
    );
    assert_eq!(
        config
            .agent_roles
            .get("reviewer")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Atlas"])
    );
    assert_eq!(
        config
            .agent_roles
            .get("writer")
            .and_then(|role| role.description.as_deref()),
        Some("from sibling")
    );
    assert_eq!(
        config
            .agent_roles
            .get("writer")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Sagan"])
    );

    Ok(())
}

#[tokio::test]
async fn mixed_legacy_and_standalone_agent_role_sources_merge_with_precedence()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let repo_root = TempDir::new()?;
    let nested_cwd = repo_root.path().join("packages").join("app");
    std::fs::create_dir_all(repo_root.path().join(".git"))?;
    std::fs::create_dir_all(&nested_cwd)?;

    let workspace_key = repo_root.path().to_string_lossy().replace('\\', "\\\\");
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        format!(
            r#"[projects."{workspace_key}"]
trust_level = "trusted"

[agents.researcher]
description = "Research role from config"
config_file = "./agents/researcher.toml"
nickname_candidates = ["Noether"]

[agents.critic]
description = "Critic role from config"
config_file = "./agents/critic.toml"
nickname_candidates = ["Ada"]
"#
        ),
    )
    .await?;

    let home_agents_dir = darwin_code_home.path().join("agents");
    tokio::fs::create_dir_all(&home_agents_dir).await?;
    tokio::fs::write(
        home_agents_dir.join("researcher.toml"),
        r#"
developer_instructions = "Research carefully"
model = "gpt-5"
"#,
    )
    .await?;
    tokio::fs::write(
        home_agents_dir.join("critic.toml"),
        r#"
developer_instructions = "Critique carefully"
model = "gpt-4.1"
"#,
    )
    .await?;

    let standalone_agents_dir = repo_root.path().join(".darwin_code").join("agents");
    tokio::fs::create_dir_all(&standalone_agents_dir).await?;
    tokio::fs::write(
        standalone_agents_dir.join("researcher.toml"),
        r#"
name = "researcher"
description = "Research role from file"
nickname_candidates = ["Hypatia"]
developer_instructions = "Research from file"
model = "gpt-5-mini"
"#,
    )
    .await?;
    tokio::fs::write(
        standalone_agents_dir.join("writer.toml"),
        r#"
name = "writer"
description = "Writer role from file"
nickname_candidates = ["Sagan"]
developer_instructions = "Write carefully"
model = "gpt-5"
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .harness_overrides(ConfigOverrides {
            cwd: Some(nested_cwd),
            ..Default::default()
        })
        .build()
        .await?;

    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.description.as_deref()),
        Some("Research role from file")
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.config_file.as_ref()),
        Some(&standalone_agents_dir.join("researcher.toml"))
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Hypatia"])
    );
    assert_eq!(
        config
            .agent_roles
            .get("critic")
            .and_then(|role| role.description.as_deref()),
        Some("Critic role from config")
    );
    assert_eq!(
        config
            .agent_roles
            .get("critic")
            .and_then(|role| role.config_file.as_ref()),
        Some(&home_agents_dir.join("critic.toml"))
    );
    assert_eq!(
        config
            .agent_roles
            .get("critic")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Ada"])
    );
    assert_eq!(
        config
            .agent_roles
            .get("writer")
            .and_then(|role| role.description.as_deref()),
        Some("Writer role from file")
    );
    assert_eq!(
        config
            .agent_roles
            .get("writer")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Sagan"])
    );

    Ok(())
}

#[tokio::test]
async fn higher_precedence_agent_role_can_inherit_description_from_lower_layer()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let repo_root = TempDir::new()?;
    let nested_cwd = repo_root.path().join("packages").join("app");
    std::fs::create_dir_all(repo_root.path().join(".git"))?;
    std::fs::create_dir_all(&nested_cwd)?;

    let workspace_key = repo_root.path().to_string_lossy().replace('\\', "\\\\");
    tokio::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        format!(
            r#"[projects."{workspace_key}"]
trust_level = "trusted"

[agents.researcher]
description = "Research role from config"
config_file = "./agents/researcher.toml"
"#
        ),
    )
    .await?;

    let home_agents_dir = darwin_code_home.path().join("agents");
    tokio::fs::create_dir_all(&home_agents_dir).await?;
    tokio::fs::write(
        home_agents_dir.join("researcher.toml"),
        r#"
developer_instructions = "Research carefully"
model = "gpt-5"
"#,
    )
    .await?;

    let standalone_agents_dir = repo_root.path().join(".darwin_code").join("agents");
    tokio::fs::create_dir_all(&standalone_agents_dir).await?;
    tokio::fs::write(
        standalone_agents_dir.join("researcher.toml"),
        r#"
name = "researcher"
nickname_candidates = ["Hypatia"]
developer_instructions = "Research from file"
model = "gpt-5-mini"
"#,
    )
    .await?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .harness_overrides(ConfigOverrides {
            cwd: Some(nested_cwd),
            ..Default::default()
        })
        .build()
        .await?;

    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.description.as_deref()),
        Some("Research role from config")
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.config_file.as_ref()),
        Some(&standalone_agents_dir.join("researcher.toml"))
    );
    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Hypatia"])
    );

    Ok(())
}

#[tokio::test]
async fn load_config_normalizes_agent_role_nickname_candidates() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let cfg = ConfigToml {
        agents: Some(AgentsToml {
            max_threads: None,
            max_depth: None,
            job_max_runtime_seconds: None,
            roles: BTreeMap::from([(
                "researcher".to_string(),
                AgentRoleToml {
                    description: Some("Research role".to_string()),
                    config_file: None,
                    nickname_candidates: Some(vec![
                        "  Hypatia  ".to_string(),
                        "Noether".to_string(),
                    ]),
                },
            )]),
        }),
        ..Default::default()
    };

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(
        config
            .agent_roles
            .get("researcher")
            .and_then(|role| role.nickname_candidates.as_ref())
            .map(|candidates| candidates.iter().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["Hypatia", "Noether"])
    );

    Ok(())
}

#[tokio::test]
async fn load_config_rejects_empty_agent_role_nickname_candidates() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let cfg = ConfigToml {
        agents: Some(AgentsToml {
            max_threads: None,
            max_depth: None,
            job_max_runtime_seconds: None,
            roles: BTreeMap::from([(
                "researcher".to_string(),
                AgentRoleToml {
                    description: Some("Research role".to_string()),
                    config_file: None,
                    nickname_candidates: Some(Vec::new()),
                },
            )]),
        }),
        ..Default::default()
    };

    let result = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await;
    let err = result.expect_err("empty nickname candidates should be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        err.to_string()
            .contains("agents.researcher.nickname_candidates")
    );

    Ok(())
}

#[tokio::test]
async fn load_config_rejects_duplicate_agent_role_nickname_candidates() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let cfg = ConfigToml {
        agents: Some(AgentsToml {
            max_threads: None,
            max_depth: None,
            job_max_runtime_seconds: None,
            roles: BTreeMap::from([(
                "researcher".to_string(),
                AgentRoleToml {
                    description: Some("Research role".to_string()),
                    config_file: None,
                    nickname_candidates: Some(vec!["Hypatia".to_string(), " Hypatia ".to_string()]),
                },
            )]),
        }),
        ..Default::default()
    };

    let result = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await;
    let err = result.expect_err("duplicate nickname candidates should be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        err.to_string()
            .contains("agents.researcher.nickname_candidates cannot contain duplicates")
    );

    Ok(())
}

#[tokio::test]
async fn load_config_rejects_unsafe_agent_role_nickname_candidates() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let cfg = ConfigToml {
        agents: Some(AgentsToml {
            max_threads: None,
            max_depth: None,
            job_max_runtime_seconds: None,
            roles: BTreeMap::from([(
                "researcher".to_string(),
                AgentRoleToml {
                    description: Some("Research role".to_string()),
                    config_file: None,
                    nickname_candidates: Some(vec!["Agent <One>".to_string()]),
                },
            )]),
        }),
        ..Default::default()
    };

    let result = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await;
    let err = result.expect_err("unsafe nickname candidates should be rejected");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(err.to_string().contains(
            "agents.researcher.nickname_candidates may only contain ASCII letters, digits, spaces, hyphens, and underscores"
        ));

    Ok(())
}

#[tokio::test]
async fn model_catalog_json_loads_from_path() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let catalog_path = darwin_code_home.path().join("catalog.json");
    let mut catalog = bundled_models_response()
        .unwrap_or_else(|err| panic!("bundled models.json should parse: {err}"));
    catalog.models = catalog.models.into_iter().take(1).collect();
    std::fs::write(
        &catalog_path,
        serde_json::to_string(&catalog).expect("serialize catalog"),
    )?;

    let cfg = ConfigToml {
        model_catalog_json: Some(catalog_path.abs()),
        ..Default::default()
    };

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(config.model_catalog, Some(catalog));
    Ok(())
}

#[tokio::test]
async fn model_catalog_json_rejects_empty_catalog() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let catalog_path = darwin_code_home.path().join("catalog.json");
    std::fs::write(&catalog_path, r#"{"models":[]}"#)?;

    let cfg = ConfigToml {
        model_catalog_json: Some(catalog_path.abs()),
        ..Default::default()
    };

    let err = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await
    .expect_err("empty custom catalog should fail config load");

    assert_eq!(err.kind(), ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("must contain at least one model"),
        "unexpected error: {err}"
    );
    Ok(())
}

fn test_cwd() -> std::io::Result<TempDir> {
    let cwd = TempDir::new()?;
    std::fs::write(cwd.path().join(".git"), "gitdir: nowhere")?;
    Ok(cwd)
}

fn write_darwin_code_source_home(root: &Path) -> std::io::Result<()> {
    std::fs::write(root.join("config.toml"), "model = \"test-model\"\n")?;
    std::fs::write(root.join("README.md"), "# Darwin Code Engine\n")?;
    std::fs::create_dir_all(root.join("codex-rs"))?;
    std::fs::write(
        root.join("codex-rs").join("Cargo.toml"),
        "[workspace]\nmembers = [\"cli\"]\n",
    )?;
    Ok(())
}

#[test]
fn find_darwin_code_home_prefers_darwin_code_home_over_codex_home() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let codex_home = TempDir::new()?;

    let resolved = find_darwin_code_home_from_env_and_cwd(
        Some(darwin_code_home.path().to_str().expect("utf-8 path")),
        Some(codex_home.path().to_str().expect("utf-8 path")),
        None,
    )?;

    assert_eq!(
        resolved.as_path(),
        darwin_code_home.path().canonicalize()?.as_path()
    );
    Ok(())
}

#[test]
fn find_darwin_code_home_uses_codex_home_when_darwin_code_home_absent() -> std::io::Result<()> {
    let codex_home = TempDir::new()?;

    let resolved = find_darwin_code_home_from_env_and_cwd(
        None,
        Some(codex_home.path().to_str().expect("utf-8 path")),
        None,
    )?;

    assert_eq!(
        resolved.as_path(),
        codex_home.path().canonicalize()?.as_path()
    );
    Ok(())
}

#[test]
fn find_darwin_code_home_detects_source_checkout_from_codex_rs_child() -> std::io::Result<()> {
    let source_home = TempDir::new()?;
    write_darwin_code_source_home(source_home.path())?;
    let child = source_home.path().join("codex-rs").join("core").join("src");
    std::fs::create_dir_all(&child)?;

    let resolved = find_darwin_code_home_from_env_and_cwd(None, None, Some(&child))?;

    assert_eq!(
        resolved.as_path(),
        source_home.path().canonicalize()?.as_path()
    );
    Ok(())
}

#[test]
fn source_checkout_detection_does_not_match_plain_project_config() -> std::io::Result<()> {
    let project = TempDir::new()?;
    std::fs::write(
        project.path().join("config.toml"),
        "model = \"test-model\"\n",
    )?;
    let child = project.path().join("src");
    std::fs::create_dir_all(&child)?;

    assert!(find_darwin_code_source_home_from_cwd(&child).is_none());
    Ok(())
}

#[tokio::test]
async fn default_config_requires_byok_provider() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "gpt-5"
"#,
    )
    .expect("cross-field BYOK validation happens during config load");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let err = Config::load_from_base_config_with_overrides_raw_for_tests(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await
    .expect_err("missing BYOK provider should fail before any OpenAI fallback");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.to_string().contains("BYOK provider is required"),
        "unexpected error: {err}"
    );
    assert!(
        err.to_string().contains("[providers.cli2api]"),
        "error should include an actionable cli2api BYOK example: {err}"
    );
    Ok(())
}

#[tokio::test]
async fn openai_compatible_provider_reads_direct_api_key() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[openai_compatible.deepseek]
base_url = "https://api.deepseek.com"
wire_api = "chat-completions"
api_key = "test-direct-key"
"#,
    )
    .expect("valid direct api_key config should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(config.model_provider_id, "deepseek");
    assert_eq!(
        config
            .model_provider
            .api_key()
            .expect("direct key resolves"),
        Some("test-direct-key".to_string())
    );
    assert_eq!(
        config
            .model_providers
            .get("deepseek")
            .expect("provider should be registered")
            .api_key()
            .expect("direct key resolves"),
        Some("test-direct-key".to_string())
    );
    assert_eq!(
        config.model_provider.base_url.as_deref(),
        Some("https://api.deepseek.com")
    );
    assert_eq!(config.model_provider.wire_api, WireApi::ChatCompletions);
    Ok(())
}

#[tokio::test]
async fn deepseek_chat_completions_openai_compatible_provider_defaults_to_chat_completions()
-> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[openai_compatible.deepseek]
base_url = "https://api.deepseek.com"
api_key = "test-direct-key"
"#,
    )
    .expect("valid direct api_key config should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(config.model_provider.wire_api, WireApi::ChatCompletions);
    Ok(())
}

#[tokio::test]
async fn openai_compatible_provider_can_select_responses_wire_api() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "gateway-model"
model_provider = "gateway"

[openai_compatible.gateway]
base_url = "http://127.0.0.1:8317/v1"
wire_api = "responses"
api_key = "test-direct-key"
"#,
    )
    .expect("valid responses-compatible config should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(config.model_provider.wire_api, WireApi::Responses);
    Ok(())
}

#[tokio::test]
async fn openai_compatible_provider_rejects_missing_direct_api_key() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[openai_compatible.deepseek]
base_url = "https://api.deepseek.com"
"#,
    )
    .expect("cross-field direct api_key validation happens during config load");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let err = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await
    .expect_err("missing direct api_key should be rejected");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        err.to_string().contains("api_key"),
        "unexpected error: {err}"
    );
    assert!(
        !err.to_string().contains("DEEPSEEK"),
        "error should not mention derived env vars: {err}"
    );
    Ok(())
}

#[tokio::test]
async fn openai_compatible_provider_rejects_blank_direct_api_key() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[openai_compatible.deepseek]
base_url = "https://api.deepseek.com"
api_key = "   "
"#,
    )
    .expect("blank direct api_key is rejected during config load");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let err = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await
    .expect_err("blank direct api_key should be rejected");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        err.to_string()
            .contains("openai_compatible.deepseek.api_key"),
        "error should include provider id and field: {err}"
    );
    assert!(
        err.to_string().contains("must not be empty"),
        "error should explain the empty field: {err}"
    );
    Ok(())
}

#[test]
fn openai_compatible_provider_rejects_removed_env_field() {
    let removed_field = format!("api_key_{}", "env");
    let toml = format!(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[openai_compatible.deepseek]
base_url = "https://api.deepseek.com"
{removed_field} = "OLD_API_KEY"
"#
    );
    let err = toml::from_str::<ConfigToml>(&toml)
        .expect_err("removed env-backed key field should be rejected by TOML parser");

    assert!(err.to_string().contains(&removed_field));
}

#[tokio::test]
async fn openai_compatible_provider_debug_redacts_direct_api_key() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[openai_compatible.deepseek]
base_url = "https://api.deepseek.com"
api_key = "test-direct-key"
"#,
    )
    .expect("valid direct api_key config should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;

    assert!(
        !format!("{:?}", config.model_provider).contains("test-direct-key"),
        "direct api_key should be redacted from debug output"
    );
    Ok(())
}

#[tokio::test]
async fn openai_compatible_provider_direct_api_key_allows_non_derivable_id() -> std::io::Result<()>
{
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "custom-model"
model_provider = "---"

[openai_compatible."---"]
base_url = "https://example.test/v1"
api_key = "test-direct-key"
"#,
    )
    .expect("valid direct api_key config should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(config.model_provider_id, "---");
    assert_eq!(
        config
            .model_provider
            .api_key()
            .expect("direct key resolves"),
        Some("test-direct-key".to_string())
    );
    Ok(())
}

#[tokio::test]
async fn byok_provider_openai_compatible_config_selects_named_provider() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[providers.deepseek]
family = "openai-compatible"
name = "DeepSeek"
base_url = "https://api.deepseek.com"
api_key = "test-direct-key"
"#,
    )
    .expect("valid BYOK provider config should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(config.model_provider_id, "deepseek");
    assert_eq!(config.model_provider.name, "DeepSeek");
    assert_eq!(
        config.model_provider.base_url.as_deref(),
        Some("https://api.deepseek.com")
    );
    assert_eq!(
        config
            .model_provider
            .api_key()
            .expect("direct key resolves"),
        Some("test-direct-key".to_string())
    );
    assert_eq!(config.model_provider.wire_api, WireApi::ChatCompletions);
    assert_eq!(
        config
            .model_providers
            .get("deepseek")
            .expect("BYOK provider should be registered"),
        &config.model_provider
    );
    Ok(())
}

#[tokio::test]
async fn byok_provider_rejects_missing_direct_api_key() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "deepseek-chat"
model_provider = "deepseek"

[providers.deepseek]
family = "openai-compatible"
base_url = "https://api.deepseek.com"
"#,
    )
    .expect("cross-field validation happens during config load");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let err = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await
    .expect_err("missing direct api_key should be rejected");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.to_string().contains("providers.deepseek"));
    assert!(err.to_string().contains("api_key"));
    Ok(())
}

#[tokio::test]
async fn byok_provider_conflict_with_advanced_provider_is_rejected() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "custom-model"
model_provider = "deepseek"

[providers.deepseek]
family = "openai-compatible"
base_url = "https://api.deepseek.com"
api_key = "test-direct-key"

[model_providers.deepseek]
name = "Advanced DeepSeek"
base_url = "https://api.deepseek.com"
"#,
    )
    .expect("same-id cross-field conflict is detected during config load");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let err = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await
    .expect_err("providers/model_providers conflict should fail");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.to_string().contains("conflicts with model_providers"));
    Ok(())
}

#[tokio::test]
async fn byok_native_provider_family_reports_runtime_gap_when_selected() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "gemini-2.5-pro"
model_provider = "gemini"

[providers.gemini]
family = "gemini-native"
name = "Google Gemini"
base_url = "https://generativelanguage.googleapis.com/v1beta"
api_key = "test-direct-key"
"#,
    )
    .expect("native-family config target should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let err = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await
    .expect_err("native runtime adapter gap should be explicit");

    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.to_string()
            .contains("native provider runtime adapter is not implemented yet"),
        "unexpected error: {err}"
    );
    Ok(())
}

#[test]
fn advanced_model_provider_rejects_non_byok_auth_surfaces() {
    for (toml, expected) in [
        (
            r#"
[model_providers.custom]
name = "Custom"
base_url = "https://example.test/v1"
experimental_bearer_token = "token"
"#,
            "experimental_bearer_token",
        ),
        (
            r#"
[model_providers.custom]
name = "Custom"
base_url = "https://example.test/v1"

[model_providers.custom.auth]
command = "./print-token"
"#,
            "command-backed auth",
        ),
    ] {
        let err = toml::from_str::<ConfigToml>(toml)
            .expect_err("advanced provider auth surface should be rejected");
        assert!(
            err.to_string().contains(expected),
            "expected `{expected}` in error, got: {err}"
        );
    }
}

#[tokio::test]
async fn openai_compatible_provider_selection_precedence_is_explicit() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
model = "custom-model"
model_provider = "openai-custom"

[openai_compatible.deepseek]
preset = "deepseek"
api_key = "test-direct-key"

[model_providers.openai-custom]
name = "OpenAI custom"
base_url = "https://custom.example/v1"
api_key = "test-direct-key"

[profiles.openai_profile]
model_provider = "openai"
"#,
    )
    .expect("valid config should parse");
    let cwd = test_cwd()?;
    let darwin_code_home = TempDir::new()?;

    let root_selected = Config::load_from_base_config_with_overrides(
        cfg.clone(),
        ConfigOverrides {
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;
    assert_eq!(root_selected.model_provider_id, "openai-custom");

    let profile_selected = Config::load_from_base_config_with_overrides(
        cfg.clone(),
        ConfigOverrides {
            config_profile: Some("openai_profile".to_string()),
            cwd: Some(cwd.path().to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;
    assert_eq!(profile_selected.model_provider_id, OPENAI_PROVIDER_ID);

    Ok(())
}

fn create_test_fixture() -> std::io::Result<PrecedenceTestFixture> {
    let toml = r#"
model = "o3"
approval_policy = "untrusted"

# Can be used to determine which profile to use if not specified by
# `ConfigOverrides`.
profile = "gpt3"


[model_providers.openai-custom]
name = "OpenAI custom"
base_url = "https://api.openai.com/v1"
api_key = "test-direct-key"
wire_api = "responses"
request_max_retries = 4            # retry failed HTTP requests
stream_max_retries = 10            # retry dropped SSE streams
stream_idle_timeout_ms = 300000    # 5m idle timeout

[profiles.o3]
model = "o3"
model_provider = "openai"
approval_policy = "never"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"

[profiles.gpt3]
model = "gpt-3.5-turbo"
model_provider = "openai-custom"

[profiles.zdr]
model = "o3"
model_provider = "openai"
approval_policy = "on-failure"


[profiles.gpt5]
model = "gpt-5.1"
model_provider = "openai"
approval_policy = "on-failure"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"
model_verbosity = "high"
"#;

    let cfg: ConfigToml = toml::from_str(toml).expect("TOML deserialization should succeed");

    // Use a temporary directory for the cwd so it does not contain an
    // AGENTS.md file.
    let cwd_temp_dir = TempDir::new().unwrap();
    let cwd = cwd_temp_dir.path().to_path_buf();
    // Make it look like a Git repo so it does not search for AGENTS.md in
    // a parent folder, either.
    std::fs::write(cwd.join(".git"), "gitdir: nowhere")?;

    let darwin_code_home_temp_dir = TempDir::new().unwrap();

    let openai_custom_provider = ModelProviderInfo {
        name: "OpenAI custom".to_string(),
        base_url: Some("https://api.openai.com/v1".to_string()),
        api_key: Some(darwin_code_model_provider_info::InlineApiKey::new(
            "test-direct-key".to_string(),
        )),
        wire_api: WireApi::Responses,
        experimental_bearer_token: None,
        auth: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(4),
        stream_max_retries: Some(10),
        stream_idle_timeout_ms: Some(300_000),
    };
    let openai_provider = ModelProviderInfo {
        name: "OpenAI".to_string(),
        base_url: Some("https://api.openai.com/v1".to_string()),
        api_key: None,
        wire_api: WireApi::Responses,
        experimental_bearer_token: None,
        auth: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
    };
    let model_provider_map = {
        let mut model_provider_map = std::collections::HashMap::new();
        model_provider_map.insert("openai".to_string(), openai_provider.clone());
        model_provider_map.insert("openai-custom".to_string(), openai_custom_provider.clone());
        model_provider_map
    };

    Ok(PrecedenceTestFixture {
        cwd: cwd_temp_dir,
        darwin_code_home: darwin_code_home_temp_dir,
        cfg,
        model_provider_map,
        openai_provider,
        openai_custom_provider,
    })
}

/// Users can specify config values at multiple levels that have the
/// following precedence:
///
/// 1. custom command-line argument, e.g. `--model o3`
/// 2. as part of a profile, where the `--profile` is specified via a CLI
///    (or in the config file itself)
/// 3. as an entry in `config.toml`, e.g. `model = "o3"`
/// 4. the default value for a required field defined in code, e.g.,
///    `crate::flags::OPENAI_DEFAULT_MODEL`
///
/// Note that profiles are the recommended way to specify a group of
/// configuration options together.
#[tokio::test]
async fn test_untrusted_project_gets_unless_trusted_approval_policy() -> anyhow::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let test_project_dir = TempDir::new()?;
    let test_path = test_project_dir.path();

    let config = Config::load_from_base_config_with_overrides(
        ConfigToml {
            projects: Some(HashMap::from([(
                test_path.to_string_lossy().to_string(),
                ProjectConfig {
                    trust_level: Some(TrustLevel::Untrusted),
                },
            )])),
            ..Default::default()
        },
        ConfigOverrides {
            cwd: Some(test_path.to_path_buf()),
            ..Default::default()
        },
        darwin_code_home.abs(),
    )
    .await?;

    // Verify that untrusted projects get UnlessTrusted approval policy
    assert_eq!(
        config.permissions.approval_policy.value(),
        AskForApproval::UnlessTrusted,
        "Expected UnlessTrusted approval policy for untrusted project"
    );

    // Verify that untrusted projects still get WorkspaceWrite sandbox (or ReadOnly on Windows)
    if cfg!(target_os = "windows") {
        assert!(
            matches!(
                config.permissions.sandbox_policy.get(),
                SandboxPolicy::ReadOnly { .. }
            ),
            "Expected ReadOnly on Windows"
        );
    } else {
        assert!(
            matches!(
                config.permissions.sandbox_policy.get(),
                SandboxPolicy::WorkspaceWrite { .. }
            ),
            "Expected WorkspaceWrite sandbox for untrusted project"
        );
    }

    Ok(())
}

#[tokio::test]
async fn requirements_disallowing_default_sandbox_falls_back_to_required_default()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_sandbox_modes: Some(vec![
                    crate::config_loader::SandboxModeRequirement::ReadOnly,
                ]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;
    assert_eq!(
        *config.permissions.sandbox_policy.get(),
        SandboxPolicy::new_read_only_policy()
    );
    Ok(())
}

#[tokio::test]
async fn explicit_sandbox_mode_falls_back_when_disallowed_by_requirements() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"sandbox_mode = "danger-full-access"
"#,
    )?;

    let requirements = crate::config_loader::ConfigRequirementsToml {
        allowed_approval_policies: None,
        allowed_approvals_reviewers: None,
        allowed_sandbox_modes: Some(vec![crate::config_loader::SandboxModeRequirement::ReadOnly]),
        allowed_web_search_modes: None,
        feature_requirements: None,
        mcp_servers: None,
        apps: None,
        rules: None,
        enforce_residency: None,
        network: None,
        permissions: None,
        guardian_policy_config: None,
    };

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async move {
            Ok(Some(requirements))
        }))
        .build()
        .await?;
    assert_eq!(
        *config.permissions.sandbox_policy.get(),
        SandboxPolicy::new_read_only_policy()
    );
    Ok(())
}

#[tokio::test]
async fn requirements_web_search_mode_overrides_danger_full_access_default() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"sandbox_mode = "danger-full-access"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_web_search_modes: Some(vec![
                    crate::config_loader::WebSearchModeRequirement::Cached,
                ]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert_eq!(config.web_search_mode.value(), WebSearchMode::Cached);
    assert_eq!(
        resolve_web_search_mode_for_turn(
            &config.web_search_mode,
            config.permissions.sandbox_policy.get(),
        ),
        WebSearchMode::Cached,
    );
    Ok(())
}

#[tokio::test]
async fn requirements_disallowing_default_approval_falls_back_to_required_default()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    let workspace = TempDir::new()?;
    let workspace_key = workspace.path().to_string_lossy().replace('\\', "\\\\");
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        format!(
            r#"
[projects."{workspace_key}"]
trust_level = "untrusted"
"#
        ),
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(workspace.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_approval_policies: Some(vec![AskForApproval::OnRequest]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert_eq!(
        config.permissions.approval_policy.value(),
        AskForApproval::OnRequest
    );
    Ok(())
}

#[tokio::test]
async fn explicit_approval_policy_falls_back_when_disallowed_by_requirements() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"approval_policy = "untrusted"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_approval_policies: Some(vec![AskForApproval::OnRequest]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;
    assert_eq!(
        config.permissions.approval_policy.value(),
        AskForApproval::OnRequest
    );
    Ok(())
}

#[tokio::test]
async fn feature_requirements_normalize_effective_feature_values() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                feature_requirements: Some(crate::config_loader::FeatureRequirementsToml {
                    entries: BTreeMap::from([
                        ("personality".to_string(), true),
                        ("shell_tool".to_string(), false),
                    ]),
                }),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert!(config.features.enabled(Feature::Personality));
    assert!(!config.features.enabled(Feature::ShellTool));
    assert!(
        !config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("Configured value for `features`")),
        "{:?}",
        config.startup_warnings
    );

    Ok(())
}

#[tokio::test]
async fn explicit_feature_config_is_normalized_by_requirements() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"
[features]
personality = false
shell_tool = true
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                feature_requirements: Some(crate::config_loader::FeatureRequirementsToml {
                    entries: BTreeMap::from([
                        ("personality".to_string(), true),
                        ("shell_tool".to_string(), false),
                    ]),
                }),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert!(config.features.enabled(Feature::Personality));
    assert!(!config.features.enabled(Feature::ShellTool));
    assert!(
        !config
            .startup_warnings
            .iter()
            .any(|warning| warning.contains("Configured value for `features`")),
        "{:?}",
        config.startup_warnings
    );

    Ok(())
}

#[tokio::test]
async fn approvals_reviewer_defaults_to_manual_only_without_guardian_feature() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert_eq!(config.approvals_reviewer, ApprovalsReviewer::User);
    Ok(())
}

#[tokio::test]
async fn prompt_instruction_blocks_can_be_disabled_from_config_and_profiles() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"include_permissions_instructions = false
include_environment_context = false
profile = "chatty"

[profiles.chatty]
include_permissions_instructions = true
include_environment_context = true
"#,
    )?;

    let config = ConfigBuilder::default()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert!(config.include_permissions_instructions);
    assert!(config.include_environment_context);
    Ok(())
}

#[tokio::test]
async fn approvals_reviewer_stays_manual_only_when_guardian_feature_is_enabled()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[features]
guardian_approval = true
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert_eq!(config.approvals_reviewer, ApprovalsReviewer::User);
    Ok(())
}

#[tokio::test]
async fn approvals_reviewer_can_be_set_in_config_without_guardian_approval() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"approvals_reviewer = "user"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert_eq!(config.approvals_reviewer, ApprovalsReviewer::User);
    Ok(())
}

#[tokio::test]
async fn approvals_reviewer_can_be_set_in_profile_without_guardian_approval() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"profile = "guardian"

[profiles.guardian]
approvals_reviewer = "guardian_subagent"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert_eq!(
        config.approvals_reviewer,
        ApprovalsReviewer::GuardianSubagent
    );
    Ok(())
}

#[tokio::test]
async fn requirements_disallowing_default_approvals_reviewer_falls_back_to_required_default()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_approvals_reviewers: Some(vec![ApprovalsReviewer::GuardianSubagent]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert_eq!(
        config.approvals_reviewer,
        ApprovalsReviewer::GuardianSubagent
    );
    Ok(())
}

#[tokio::test]
async fn root_approvals_reviewer_falls_back_when_disallowed_by_requirements() -> std::io::Result<()>
{
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"approvals_reviewer = "user"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_approvals_reviewers: Some(vec![ApprovalsReviewer::GuardianSubagent]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert_eq!(
        config.approvals_reviewer,
        ApprovalsReviewer::GuardianSubagent
    );
    assert!(
        config.startup_warnings.iter().any(|warning| {
            warning
                .contains("Configured value for `approvals_reviewer` is disallowed by requirements")
        }),
        "{:?}",
        config.startup_warnings
    );
    Ok(())
}

#[tokio::test]
async fn profile_approvals_reviewer_falls_back_when_disallowed_by_requirements()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"profile = "default"

[profiles.default]
approvals_reviewer = "user"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_approvals_reviewers: Some(vec![ApprovalsReviewer::GuardianSubagent]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert_eq!(
        config.approvals_reviewer,
        ApprovalsReviewer::GuardianSubagent
    );
    Ok(())
}

#[tokio::test]
async fn approvals_reviewer_preserves_valid_user_choice_when_allowed_by_requirements()
-> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"approvals_reviewer = "guardian_subagent"
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                allowed_approvals_reviewers: Some(vec![
                    ApprovalsReviewer::User,
                    ApprovalsReviewer::GuardianSubagent,
                ]),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    assert_eq!(
        config.approvals_reviewer,
        ApprovalsReviewer::GuardianSubagent
    );
    assert!(
        config
            .startup_warnings
            .iter()
            .all(|warning| !warning.contains("approvals_reviewer")),
        "{:?}",
        config.startup_warnings
    );
    Ok(())
}

#[tokio::test]
async fn smart_approvals_alias_is_ignored() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[features]
smart_approvals = true
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert!(!config.features.enabled(Feature::GuardianApproval));
    assert_eq!(config.approvals_reviewer, ApprovalsReviewer::User);

    let serialized =
        tokio::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE)).await?;
    assert!(serialized.contains("smart_approvals = true"));
    assert!(!serialized.contains("guardian_approval"));
    assert!(!serialized.contains("approvals_reviewer"));

    Ok(())
}

#[tokio::test]
async fn smart_approvals_alias_is_ignored_in_profiles() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"profile = "guardian"

[profiles.guardian.features]
smart_approvals = true
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert!(!config.features.enabled(Feature::GuardianApproval));
    assert_eq!(config.approvals_reviewer, ApprovalsReviewer::User);

    let serialized =
        tokio::fs::read_to_string(darwin_code_home.path().join(CONFIG_TOML_FILE)).await?;
    assert!(serialized.contains("[profiles.guardian.features]"));
    assert!(serialized.contains("smart_approvals = true"));
    assert!(!serialized.contains("guardian_approval"));
    assert!(!serialized.contains("approvals_reviewer"));

    Ok(())
}

#[tokio::test]
async fn multi_agent_v2_config_from_feature_table() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"[features.multi_agent_v2]
enabled = true
usage_hint_enabled = false
usage_hint_text = "Custom delegation guidance."
hide_spawn_agent_metadata = true
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert!(config.features.enabled(Feature::MultiAgentV2));
    assert!(!config.multi_agent_v2.usage_hint_enabled);
    assert_eq!(
        config.multi_agent_v2.usage_hint_text.as_deref(),
        Some("Custom delegation guidance.")
    );
    assert!(config.multi_agent_v2.hide_spawn_agent_metadata);

    Ok(())
}

#[tokio::test]
async fn profile_multi_agent_v2_config_overrides_base() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;
    std::fs::write(
        darwin_code_home.path().join(CONFIG_TOML_FILE),
        r#"profile = "no_hint"

[features.multi_agent_v2]
usage_hint_enabled = true
usage_hint_text = "base hint"
hide_spawn_agent_metadata = true

[profiles.no_hint.features.multi_agent_v2]
usage_hint_enabled = false
usage_hint_text = "profile hint"
hide_spawn_agent_metadata = false
"#,
    )?;

    let config = ConfigBuilder::without_managed_config_for_tests()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .fallback_cwd(Some(darwin_code_home.path().to_path_buf()))
        .build()
        .await?;

    assert!(!config.multi_agent_v2.usage_hint_enabled);
    assert_eq!(
        config.multi_agent_v2.usage_hint_text.as_deref(),
        Some("profile hint")
    );
    assert!(!config.multi_agent_v2.hide_spawn_agent_metadata);

    Ok(())
}

#[tokio::test]
async fn feature_requirements_normalize_runtime_feature_mutations() -> std::io::Result<()> {
    let darwin_code_home = TempDir::new()?;

    let mut config = ConfigBuilder::default()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                feature_requirements: Some(crate::config_loader::FeatureRequirementsToml {
                    entries: BTreeMap::from([
                        ("personality".to_string(), true),
                        ("shell_tool".to_string(), false),
                    ]),
                }),
                ..Default::default()
            }))
        }))
        .build()
        .await?;

    let mut requested = config.features.get().clone();
    requested
        .disable(Feature::Personality)
        .enable(Feature::ShellTool);
    assert!(config.features.can_set(&requested).is_ok());
    config
        .features
        .set(requested)
        .expect("managed feature mutations should normalize successfully");

    assert!(config.features.enabled(Feature::Personality));
    assert!(!config.features.enabled(Feature::ShellTool));

    Ok(())
}

#[tokio::test]
async fn feature_requirements_reject_collab_legacy_alias() {
    let darwin_code_home = TempDir::new().expect("tempdir");

    let err = ConfigBuilder::default()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .external_requirements(ExternalRequirementsLoader::new(async {
            Ok(Some(crate::config_loader::ConfigRequirementsToml {
                feature_requirements: Some(crate::config_loader::FeatureRequirementsToml {
                    entries: BTreeMap::from([("collab".to_string(), true)]),
                }),
                ..Default::default()
            }))
        }))
        .build()
        .await
        .expect_err("legacy aliases should be rejected");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string()
            .contains("use canonical feature key `multi_agent`"),
        "{err}"
    );
}

#[tokio::test]
async fn tool_suggest_discoverables_load_from_config_toml() -> std::io::Result<()> {
    let cfg: ConfigToml = toml::from_str(
        r#"
[tool_suggest]
discoverables = [
  { type = "connector", id = "connector_alpha" },
  { type = "plugin", id = "plugin_alpha@openai-curated" },
  { type = "connector", id = "   " }
]
"#,
    )
    .expect("TOML deserialization should succeed");

    assert_eq!(
        cfg.tool_suggest,
        Some(ToolSuggestConfig {
            discoverables: vec![
                ToolSuggestDiscoverable {
                    kind: ToolSuggestDiscoverableType::Connector,
                    id: "connector_alpha".to_string(),
                },
                ToolSuggestDiscoverable {
                    kind: ToolSuggestDiscoverableType::Plugin,
                    id: "plugin_alpha@openai-curated".to_string(),
                },
                ToolSuggestDiscoverable {
                    kind: ToolSuggestDiscoverableType::Connector,
                    id: "   ".to_string(),
                },
            ],
        })
    );

    let darwin_code_home = TempDir::new()?;
    let config = Config::load_from_base_config_with_overrides(
        cfg,
        ConfigOverrides::default(),
        darwin_code_home.abs(),
    )
    .await?;

    assert_eq!(
        config.tool_suggest,
        ToolSuggestConfig {
            discoverables: vec![
                ToolSuggestDiscoverable {
                    kind: ToolSuggestDiscoverableType::Connector,
                    id: "connector_alpha".to_string(),
                },
                ToolSuggestDiscoverable {
                    kind: ToolSuggestDiscoverableType::Plugin,
                    id: "plugin_alpha@openai-curated".to_string(),
                },
            ],
        }
    );
    Ok(())
}

#[derive(Deserialize, Debug, PartialEq)]
struct TuiTomlTest {
    #[serde(default, flatten)]
    notifications: TuiNotificationSettings,
}

#[derive(Deserialize, Debug, PartialEq)]
struct RootTomlTest {
    tui: TuiTomlTest,
}

#[test]
fn test_tui_notifications_true() {
    let toml = r#"
            [tui]
            notifications = true
        "#;
    let parsed: RootTomlTest = toml::from_str(toml).expect("deserialize notifications=true");
    assert_matches!(
        parsed.tui.notifications.notifications,
        Notifications::Enabled(true)
    );
}

#[test]
fn test_tui_notifications_custom_array() {
    let toml = r#"
            [tui]
            notifications = ["foo"]
        "#;
    let parsed: RootTomlTest = toml::from_str(toml).expect("deserialize notifications=[\"foo\"]");
    assert_matches!(
        parsed.tui.notifications.notifications,
        Notifications::Custom(ref v) if v == &vec!["foo".to_string()]
    );
}

#[test]
fn test_tui_notification_method() {
    let toml = r#"
            [tui]
            notification_method = "bel"
        "#;
    let parsed: RootTomlTest =
        toml::from_str(toml).expect("deserialize notification_method=\"bel\"");
    assert_eq!(parsed.tui.notifications.method, NotificationMethod::Bel);
}

#[test]
fn test_tui_notification_condition_defaults_to_unfocused() {
    let toml = r#"
            [tui]
        "#;
    let parsed: RootTomlTest =
        toml::from_str(toml).expect("deserialize default notification condition");
    assert_eq!(
        parsed.tui.notifications.condition,
        NotificationCondition::Unfocused
    );
}

#[test]
fn test_tui_notification_condition_always() {
    let toml = r#"
            [tui]
            notification_condition = "always"
        "#;
    let parsed: RootTomlTest =
        toml::from_str(toml).expect("deserialize notification_condition=\"always\"");
    assert_eq!(
        parsed.tui.notifications.condition,
        NotificationCondition::Always
    );
}

#[test]
fn test_tui_notification_condition_rejects_unknown_value() {
    let toml = r#"
            [tui]
            notification_condition = "background"
        "#;
    let err = toml::from_str::<RootTomlTest>(toml).expect_err("reject unknown condition");
    let err = err.to_string();
    assert!(
        err.contains("unknown variant `background`")
            && err.contains("unfocused")
            && err.contains("always"),
        "unexpected error: {err}"
    );
}
