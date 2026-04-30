#![expect(clippy::expect_used)]

use anyhow::Context as _;
use anyhow::ensure;
use ctor::ctor;
use darwin_code_arg0::Arg0PathEntryGuard;
use darwin_code_utils_cargo_bin::CargoBinError;
use std::sync::OnceLock;
use tempfile::TempDir;

use darwin_code_core::DarwinCodeThread;
use darwin_code_core::config::Config;
use darwin_code_core::config::ConfigBuilder;
use darwin_code_core::config::ConfigOverrides;
use darwin_code_utils_absolute_path::AbsolutePathBuf;
pub use darwin_code_utils_absolute_path::test_support::PathBufExt;
pub use darwin_code_utils_absolute_path::test_support::PathExt;
use regex_lite::Regex;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ByokTestAuth;

impl ByokTestAuth {
    pub fn from_api_key(_api_key: impl Into<String>) -> Self {
        Self
    }

    pub fn dummy_for_testing() -> Self {
        Self
    }
}

pub const DARWIN_CODE_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";
pub const OPENAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";

const DEFAULT_BYOK_TEST_CONFIG: &str = r#"
[providers.openai]
family = "openai-compatible"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
api_key = "test-direct-api-key"
"#;

pub fn write_default_byok_test_config(darwin_code_home: &std::path::Path) {
    ensure_default_byok_provider_config_path(darwin_code_home);
}

pub mod apps_test_server;
pub mod context_snapshot;
pub mod process;
pub mod responses;
pub mod streaming_sse;
#[path = "test_codex.rs"]
pub mod test_darwin_code;
#[path = "test_codex_exec.rs"]
pub mod test_darwin_code_exec;
pub mod tracing;
pub mod zsh_fork;

static TEST_ARG0_PATH_ENTRY: OnceLock<Option<Arg0PathEntryGuard>> = OnceLock::new();

#[ctor]
fn enable_deterministic_unified_exec_process_ids_for_tests() {
    darwin_code_core::test_support::set_thread_manager_test_mode(/*enabled*/ true);
    darwin_code_core::test_support::set_deterministic_process_ids(/*enabled*/ true);
}

#[ctor]
fn configure_localhost_no_proxy_for_tests() {
    let local_hosts = "localhost,127.0.0.1,::1";
    for key in ["NO_PROXY", "no_proxy"] {
        match std::env::var(key) {
            Ok(existing) if existing.contains("127.0.0.1") && existing.contains("localhost") => {}
            Ok(existing) if existing.trim().is_empty() => unsafe {
                std::env::set_var(key, local_hosts);
            },
            Ok(existing) => unsafe {
                std::env::set_var(key, format!("{existing},{local_hosts}"));
            },
            Err(_) => unsafe {
                std::env::set_var(key, local_hosts);
            },
        }
    }
}

#[ctor]
fn configure_arg0_dispatch_for_test_binaries() {
    let _ = TEST_ARG0_PATH_ENTRY.get_or_init(darwin_code_arg0::arg0_dispatch);
}

#[ctor]
fn configure_default_byok_api_key_for_tests() {
    if std::env::var_os(OPENAI_API_KEY_ENV_VAR).is_some() {
        return;
    }
    // Safety: this ctor runs at process startup before test threads begin.
    unsafe {
        std::env::set_var(OPENAI_API_KEY_ENV_VAR, "Test API Key");
    }
}

#[ctor]
fn configure_insta_workspace_root_for_snapshot_tests() {
    if std::env::var_os("INSTA_WORKSPACE_ROOT").is_some() {
        return;
    }

    let workspace_root = darwin_code_utils_cargo_bin::repo_root()
        .ok()
        .map(|root| root.join("darwin_code-rs"));

    if let Some(workspace_root) = workspace_root
        && let Ok(workspace_root) = workspace_root.canonicalize()
    {
        // Safety: this ctor runs at process startup before test threads begin.
        unsafe {
            std::env::set_var("INSTA_WORKSPACE_ROOT", workspace_root);
        }
    }
}

#[track_caller]
pub fn assert_regex_match<'s>(pattern: &str, actual: &'s str) -> regex_lite::Captures<'s> {
    let regex = Regex::new(pattern).unwrap_or_else(|err| {
        panic!("failed to compile regex {pattern:?}: {err}");
    });
    regex
        .captures(actual)
        .unwrap_or_else(|| panic!("regex {pattern:?} did not match {actual:?}"))
}

pub fn test_path_buf_with_windows(unix_path: &str, windows_path: Option<&str>) -> PathBuf {
    if cfg!(windows) {
        if let Some(windows) = windows_path {
            PathBuf::from(windows)
        } else {
            let mut path = PathBuf::from(r"C:\");
            path.extend(
                unix_path
                    .trim_start_matches('/')
                    .split('/')
                    .filter(|segment| !segment.is_empty()),
            );
            path
        }
    } else {
        PathBuf::from(unix_path)
    }
}

pub fn test_path_buf(unix_path: &str) -> PathBuf {
    test_path_buf_with_windows(unix_path, /*windows_path*/ None)
}

pub fn test_absolute_path_with_windows(
    unix_path: &str,
    windows_path: Option<&str>,
) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(test_path_buf_with_windows(unix_path, windows_path))
        .expect("test path should be absolute")
}

pub fn test_absolute_path(unix_path: &str) -> AbsolutePathBuf {
    test_absolute_path_with_windows(unix_path, /*windows_path*/ None)
}

pub trait TempDirExt {
    fn abs(&self) -> AbsolutePathBuf;
}

impl TempDirExt for TempDir {
    fn abs(&self) -> AbsolutePathBuf {
        self.path().abs()
    }
}

pub fn test_tmp_path() -> AbsolutePathBuf {
    test_absolute_path_with_windows("/tmp", Some(r"C:\Users\darwin_code\AppData\Local\Temp"))
}

pub fn test_tmp_path_buf() -> PathBuf {
    test_tmp_path().into_path_buf()
}

/// Fetch a DotSlash resource and return the resolved executable/file path.
pub fn fetch_dotslash_file(
    dotslash_file: &std::path::Path,
    dotslash_cache: Option<&std::path::Path>,
) -> anyhow::Result<PathBuf> {
    let mut command = std::process::Command::new("dotslash");
    command.arg("--").arg("fetch").arg(dotslash_file);
    if let Some(dotslash_cache) = dotslash_cache {
        command.env("DOTSLASH_CACHE", dotslash_cache);
    }
    let output = command.output().with_context(|| {
        format!(
            "failed to run dotslash to fetch resource {}",
            dotslash_file.display()
        )
    })?;
    ensure!(
        output.status.success(),
        "dotslash fetch failed for {}: {}",
        dotslash_file.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let fetched_path = String::from_utf8(output.stdout)
        .context("dotslash fetch output was not utf8")?
        .trim()
        .to_string();
    ensure!(!fetched_path.is_empty(), "dotslash fetch output was empty");
    let fetched_path = PathBuf::from(fetched_path);
    ensure!(
        fetched_path.is_file(),
        "dotslash returned non-file path: {}",
        fetched_path.display()
    );
    Ok(fetched_path)
}

/// Returns a default `Config` whose on-disk state is confined to the provided
/// temporary directory. Using a per-test directory keeps tests hermetic and
/// avoids clobbering a developer’s real `~/.darwin_code`.
pub async fn load_default_config_for_test(darwin_code_home: &TempDir) -> Config {
    ensure_default_byok_provider_config(darwin_code_home);
    ConfigBuilder::default()
        .darwin_code_home(darwin_code_home.path().to_path_buf())
        .harness_overrides(default_test_overrides())
        .build()
        .await
        .expect("defaults for test should always succeed")
}

fn ensure_default_byok_provider_config(darwin_code_home: &TempDir) {
    ensure_default_byok_provider_config_path(darwin_code_home.path());
}

fn ensure_default_byok_provider_config_path(darwin_code_home: &std::path::Path) {
    let config_path = darwin_code_home.join("config.toml");
    let mut contents = fs::read_to_string(&config_path).unwrap_or_default();
    if contents.contains("[providers.") || contents.contains("[openai_compatible_provider]") {
        return;
    }
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(DEFAULT_BYOK_TEST_CONFIG);
    fs::write(config_path, contents).expect("write default BYOK test config");
}

#[cfg(target_os = "linux")]
fn default_test_overrides() -> ConfigOverrides {
    ConfigOverrides {
        codex_linux_sandbox_exe: Some(
            find_darwin_code_linux_sandbox_exe()
                .expect("should find binary for darwin_code-linux-sandbox"),
        ),
        ..ConfigOverrides::default()
    }
}

#[cfg(not(target_os = "linux"))]
fn default_test_overrides() -> ConfigOverrides {
    ConfigOverrides::default()
}

#[cfg(target_os = "linux")]
pub fn find_darwin_code_linux_sandbox_exe() -> Result<PathBuf, CargoBinError> {
    if let Some(path) = TEST_ARG0_PATH_ENTRY
        .get()
        .and_then(Option::as_ref)
        .and_then(|path_entry| path_entry.paths().darwin_code_linux_sandbox_exe.clone())
    {
        return Ok(path);
    }

    if let Ok(path) = std::env::current_exe() {
        return Ok(path);
    }

    darwin_code_utils_cargo_bin::cargo_bin("darwin_code-linux-sandbox")
}

/// Builds an SSE stream body from a JSON fixture.
///
/// The fixture must contain an array of objects where each object represents a
/// single SSE event with at least a `type` field matching the `event:` value.
/// Additional fields become the JSON payload for the `data:` line. An object
/// with only a `type` field results in an event with no `data:` section. This
/// makes it trivial to extend the fixtures as OpenAI adds new event kinds or
/// fields.
pub fn load_sse_fixture(path: impl AsRef<std::path::Path>) -> String {
    let events: Vec<serde_json::Value> =
        serde_json::from_reader(std::fs::File::open(path).expect("read fixture"))
            .expect("parse JSON fixture");
    events
        .into_iter()
        .map(|e| {
            let kind = e
                .get("type")
                .and_then(|v| v.as_str())
                .expect("fixture event missing type");
            if e.as_object().map(|o| o.len() == 1).unwrap_or(false) {
                format!("event: {kind}\n\n")
            } else {
                format!("event: {kind}\ndata: {e}\n\n")
            }
        })
        .collect()
}

pub fn load_sse_fixture_with_id_from_str(raw: &str, id: &str) -> String {
    let replaced = raw.replace("__ID__", id);
    let events: Vec<serde_json::Value> =
        serde_json::from_str(&replaced).expect("parse JSON fixture");
    events
        .into_iter()
        .map(|e| {
            let kind = e
                .get("type")
                .and_then(|v| v.as_str())
                .expect("fixture event missing type");
            if e.as_object().map(|o| o.len() == 1).unwrap_or(false) {
                format!("event: {kind}\n\n")
            } else {
                format!("event: {kind}\ndata: {e}\n\n")
            }
        })
        .collect()
}

pub async fn wait_for_event<F>(
    darwin_code: &DarwinCodeThread,
    predicate: F,
) -> darwin_code_protocol::protocol::EventMsg
where
    F: FnMut(&darwin_code_protocol::protocol::EventMsg) -> bool,
{
    use tokio::time::Duration;
    wait_for_event_with_timeout(darwin_code, predicate, Duration::from_secs(1)).await
}

pub async fn wait_for_event_match<T, F>(darwin_code: &DarwinCodeThread, matcher: F) -> T
where
    F: Fn(&darwin_code_protocol::protocol::EventMsg) -> Option<T>,
{
    let ev = wait_for_event(darwin_code, |ev| matcher(ev).is_some()).await;
    matcher(&ev).expect("EventMsg should match matcher predicate")
}

pub async fn wait_for_event_with_timeout<F>(
    darwin_code: &DarwinCodeThread,
    mut predicate: F,
    wait_time: tokio::time::Duration,
) -> darwin_code_protocol::protocol::EventMsg
where
    F: FnMut(&darwin_code_protocol::protocol::EventMsg) -> bool,
{
    use tokio::time::Duration;
    use tokio::time::timeout;
    loop {
        // Allow a bit more time to accommodate async startup work (e.g. config IO, tool discovery)
        let ev = timeout(
            wait_time.max(Duration::from_secs(10)),
            darwin_code.next_event(),
        )
        .await
        .expect("timeout waiting for event")
        .expect("stream ended unexpectedly");
        if predicate(&ev.msg) {
            return ev.msg;
        }
    }
}

pub fn sandbox_env_var() -> &'static str {
    darwin_code_core::spawn::DARWIN_CODE_SANDBOX_ENV_VAR
}

pub fn sandbox_network_env_var() -> &'static str {
    darwin_code_core::spawn::DARWIN_CODE_SANDBOX_NETWORK_DISABLED_ENV_VAR
}

const REMOTE_ENV_ENV_VAR: &str = "DARWIN_CODE_TEST_REMOTE_ENV";

pub fn remote_env_env_var() -> &'static str {
    REMOTE_ENV_ENV_VAR
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteEnvConfig {
    pub container_name: String,
}

pub fn get_remote_test_env() -> Option<RemoteEnvConfig> {
    if std::env::var_os(REMOTE_ENV_ENV_VAR).is_none() {
        eprintln!("Skipping test because {REMOTE_ENV_ENV_VAR} is not set.");
        return None;
    }

    let container_name = std::env::var(REMOTE_ENV_ENV_VAR)
        .unwrap_or_else(|_| panic!("{REMOTE_ENV_ENV_VAR} must be set"));
    assert!(
        !container_name.trim().is_empty(),
        "{REMOTE_ENV_ENV_VAR} must not be empty"
    );

    Some(RemoteEnvConfig { container_name })
}

pub fn format_with_current_shell(command: &str) -> Vec<String> {
    darwin_code_core::shell::default_user_shell()
        .derive_exec_args(command, /*use_login_shell*/ true)
}

pub fn format_with_current_shell_display(command: &str) -> String {
    let args = format_with_current_shell(command);
    shlex::try_join(args.iter().map(String::as_str)).expect("serialize current shell command")
}

pub fn format_with_current_shell_non_login(command: &str) -> Vec<String> {
    darwin_code_core::shell::default_user_shell()
        .derive_exec_args(command, /*use_login_shell*/ false)
}

pub fn format_with_current_shell_display_non_login(command: &str) -> String {
    let args = format_with_current_shell_non_login(command);
    shlex::try_join(args.iter().map(String::as_str))
        .expect("serialize current shell command without login")
}

pub fn stdio_server_bin() -> Result<String, CargoBinError> {
    darwin_code_utils_cargo_bin::cargo_bin("test_stdio_server")
        .map(|p| p.to_string_lossy().to_string())
}

pub mod fs_wait {
    use anyhow::Result;
    use anyhow::anyhow;
    use notify::RecursiveMode;
    use notify::Watcher;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Duration;
    use std::time::Instant;
    use tokio::task;
    use walkdir::WalkDir;

    pub async fn wait_for_path_exists(
        path: impl Into<PathBuf>,
        timeout: Duration,
    ) -> Result<PathBuf> {
        let path = path.into();
        task::spawn_blocking(move || wait_for_path_exists_blocking(path, timeout)).await?
    }

    pub async fn wait_for_matching_file(
        root: impl Into<PathBuf>,
        timeout: Duration,
        predicate: impl FnMut(&Path) -> bool + Send + 'static,
    ) -> Result<PathBuf> {
        let root = root.into();
        task::spawn_blocking(move || {
            let mut predicate = predicate;
            blocking_find_matching_file(root, timeout, &mut predicate)
        })
        .await?
    }

    fn wait_for_path_exists_blocking(path: PathBuf, timeout: Duration) -> Result<PathBuf> {
        if path.exists() {
            return Ok(path);
        }

        let watch_root = nearest_existing_ancestor(&path);
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;
        watcher.watch(&watch_root, RecursiveMode::Recursive)?;

        let deadline = Instant::now() + timeout;
        loop {
            if path.exists() {
                return Ok(path);
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(now);
            match rx.recv_timeout(remaining) {
                Ok(Ok(_event)) => {
                    if path.exists() {
                        return Ok(path);
                    }
                }
                Ok(Err(err)) => return Err(err.into()),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        if path.exists() {
            Ok(path)
        } else {
            Err(anyhow!("timed out waiting for {path:?}"))
        }
    }

    fn blocking_find_matching_file(
        root: PathBuf,
        timeout: Duration,
        predicate: &mut impl FnMut(&Path) -> bool,
    ) -> Result<PathBuf> {
        let root = wait_for_path_exists_blocking(root, timeout)?;

        if let Some(found) = scan_for_match(&root, predicate) {
            return Ok(found);
        }

        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;

        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(Ok(_event)) => {
                    if let Some(found) = scan_for_match(&root, predicate) {
                        return Ok(found);
                    }
                }
                Ok(Err(err)) => return Err(err.into()),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        if let Some(found) = scan_for_match(&root, predicate) {
            Ok(found)
        } else {
            Err(anyhow!("timed out waiting for matching file in {root:?}"))
        }
    }

    fn scan_for_match(root: &Path, predicate: &mut impl FnMut(&Path) -> bool) -> Option<PathBuf> {
        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if !entry.file_type().is_file() {
                continue;
            }
            if predicate(path) {
                return Some(path.to_path_buf());
            }
        }
        None
    }

    fn nearest_existing_ancestor(path: &Path) -> PathBuf {
        let mut current = path;
        loop {
            if current.exists() {
                return current.to_path_buf();
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => return PathBuf::from("."),
            }
        }
    }
}

#[macro_export]
macro_rules! skip_if_sandbox {
    () => {{
        if ::std::env::var($crate::sandbox_env_var())
            == ::core::result::Result::Ok("seatbelt".to_string())
        {
            eprintln!(
                "{} is set to 'seatbelt', skipping test.",
                $crate::sandbox_env_var()
            );
            return;
        }
    }};
    ($return_value:expr $(,)?) => {{
        if ::std::env::var($crate::sandbox_env_var())
            == ::core::result::Result::Ok("seatbelt".to_string())
        {
            eprintln!(
                "{} is set to 'seatbelt', skipping test.",
                $crate::sandbox_env_var()
            );
            return $return_value;
        }
    }};
}

#[macro_export]
macro_rules! skip_if_no_network {
    () => {{
        if ::std::env::var($crate::sandbox_network_env_var()).is_ok() {
            println!(
                "Skipping test because it cannot execute when network is disabled in a DarwinCode sandbox."
            );
            return;
        }
    }};
    ($return_value:expr $(,)?) => {{
        if ::std::env::var($crate::sandbox_network_env_var()).is_ok() {
            println!(
                "Skipping test because it cannot execute when network is disabled in a DarwinCode sandbox."
            );
            return $return_value;
        }
    }};
}

#[macro_export]
macro_rules! skip_if_remote {
    ($reason:expr $(,)?) => {{
        if ::std::env::var_os($crate::remote_env_env_var()).is_some() {
            eprintln!(
                "Skipping test under {}: {}",
                $crate::remote_env_env_var(),
                $reason
            );
            return;
        }
    }};
    ($return_value:expr, $reason:expr $(,)?) => {{
        if ::std::env::var_os($crate::remote_env_env_var()).is_some() {
            eprintln!(
                "Skipping test under {}: {}",
                $crate::remote_env_env_var(),
                $reason
            );
            return $return_value;
        }
    }};
}

#[macro_export]
macro_rules! darwin_code_linux_sandbox_exe_or_skip {
    () => {{
        #[cfg(target_os = "linux")]
        {
            match $crate::find_darwin_code_linux_sandbox_exe() {
                Ok(path) => Some(path),
                Err(err) => {
                    eprintln!(
                        "darwin_code-linux-sandbox binary not available, skipping test: {err}"
                    );
                    return;
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }};
    ($return_value:expr $(,)?) => {{
        #[cfg(target_os = "linux")]
        {
            match $crate::find_darwin_code_linux_sandbox_exe() {
                Ok(path) => Some(path),
                Err(err) => {
                    eprintln!(
                        "darwin_code-linux-sandbox binary not available, skipping test: {err}"
                    );
                    return $return_value;
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }};
}

#[macro_export]
macro_rules! skip_if_windows {
    ($return_value:expr $(,)?) => {{
        if cfg!(target_os = "windows") {
            println!("Skipping test because it cannot execute on Windows.");
            return $return_value;
        }
    }};
}
