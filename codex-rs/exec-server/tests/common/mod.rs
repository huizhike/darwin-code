use std::path::PathBuf;

pub mod exec_server;

pub(crate) fn current_test_binary_helper_paths() -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
    Ok((
        darwin_code_utils_cargo_bin::cargo_bin("darwin_code")?,
        Some(darwin_code_utils_cargo_bin::cargo_bin(
            "darwin_code-linux-sandbox",
        )?),
    ))
}
