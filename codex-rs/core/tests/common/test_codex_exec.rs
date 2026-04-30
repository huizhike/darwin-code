#![allow(clippy::expect_used)]
use crate::DARWIN_CODE_API_KEY_ENV_VAR;
use crate::write_default_byok_test_config;
use std::path::Path;
use tempfile::TempDir;
use wiremock::MockServer;

pub struct TestDarwinCodeExecBuilder {
    home: TempDir,
    cwd: TempDir,
}

impl TestDarwinCodeExecBuilder {
    pub fn cmd(&self) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::new(
            darwin_code_utils_cargo_bin::cargo_bin("darwin_code-exec")
                .expect("should find binary for darwin_code-exec"),
        );
        cmd.current_dir(self.cwd.path())
            .env("DARWIN_CODE_HOME", self.home.path())
            .env("DARWIN_CODE_SQLITE_HOME", self.home.path())
            .env(DARWIN_CODE_API_KEY_ENV_VAR, "dummy");
        cmd
    }
    pub fn cmd_with_server(&self, server: &MockServer) -> assert_cmd::Command {
        let mut cmd = self.cmd();
        let base = format!("{}/v1", server.uri());
        cmd.arg("-c").arg(format!(
            "providers.openai.base_url={}",
            toml_string_literal(&base)
        ));
        cmd
    }

    pub fn cwd_path(&self) -> &Path {
        self.cwd.path()
    }
    pub fn home_path(&self) -> &Path {
        self.home.path()
    }
}

fn toml_string_literal(value: &str) -> String {
    serde_json::to_string(value).expect("serialize TOML string literal")
}

pub fn test_darwin_code_exec() -> TestDarwinCodeExecBuilder {
    let home = TempDir::new().expect("create temp home");
    write_default_byok_test_config(home.path());
    TestDarwinCodeExecBuilder {
        home,
        cwd: TempDir::new().expect("create temp cwd"),
    }
}
