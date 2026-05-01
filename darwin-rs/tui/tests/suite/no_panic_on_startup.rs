use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::select;
use tokio::time::timeout;

/// Regression test for the startup no-panic regression.
#[tokio::test]
#[ignore = "TODO(mbolin): flaky"]
async fn malformed_rules_should_not_panic() -> anyhow::Result<()> {
    // run_darwin_code_cli() does not work on Windows due to PTY limitations.
    if cfg!(windows) {
        return Ok(());
    }

    let tmp = tempfile::tempdir()?;
    let darwin_code_home = tmp.path();
    std::fs::write(
        darwin_code_home.join("rules"),
        "rules should be a directory not a file",
    )?;

    // TODO(mbolin): Figure out why using a temp dir as the cwd causes this test
    // to hang.
    let cwd = std::env::current_dir()?;
    let config_contents = format!(
        r#"
# Pick a BYOK provider so the CLI stays in local config mode for this test.
model_provider = "openai"

[providers.openai]
family = "openai-compatible"
name = "OpenAI"
base_url = "https://api.openai.com/v1"
api_key = "test-direct-api-key"

[projects]
"{cwd}" = {{ trust_level = "trusted" }}
"#,
        cwd = cwd.display()
    );
    std::fs::write(darwin_code_home.join("config.toml"), config_contents)?;

    let DarwinCodeCliOutput { exit_code, output } =
        run_darwin_code_cli(darwin_code_home, cwd).await?;
    assert_ne!(0, exit_code, "DarwinCode CLI should exit nonzero.");
    assert!(
        output.contains("ERROR: Failed to initialize darwin_code:"),
        "expected startup error in output, got: {output}"
    );
    assert!(
        output.contains("failed to read rules files"),
        "expected rules read error in output, got: {output}"
    );
    Ok(())
}

struct DarwinCodeCliOutput {
    exit_code: i32,
    output: String,
}

async fn run_darwin_code_cli(
    darwin_code_home: impl AsRef<Path>,
    cwd: impl AsRef<Path>,
) -> anyhow::Result<DarwinCodeCliOutput> {
    let darwin_code_cli = darwin_code_utils_cargo_bin::cargo_bin("darwin_code")?;
    let mut env = HashMap::new();
    env.insert(
        "DARWIN_CODE_HOME".to_string(),
        darwin_code_home.as_ref().display().to_string(),
    );

    let args = vec!["-c".to_string(), "analytics.enabled=false".to_string()];
    let spawned = darwin_code_utils_pty::spawn_pty_process(
        darwin_code_cli.to_string_lossy().as_ref(),
        &args,
        cwd.as_ref(),
        &env,
        &None,
        darwin_code_utils_pty::TerminalSize::default(),
    )
    .await?;
    let mut output = Vec::new();
    let darwin_code_utils_pty::SpawnedProcess {
        session,
        stdout_rx,
        stderr_rx,
        exit_rx,
    } = spawned;
    let mut output_rx = darwin_code_utils_pty::combine_output_receivers(stdout_rx, stderr_rx);
    let mut exit_rx = exit_rx;
    let writer_tx = session.writer_sender();
    let exit_code_result = timeout(Duration::from_secs(10), async {
        // Read PTY output until the process exits while replying to cursor
        // position queries so the TUI can initialize without a real terminal.
        loop {
            select! {
                result = output_rx.recv() => match result {
                    Ok(chunk) => {
                        // The TUI asks for the cursor position via ESC[6n.
                        // Respond with a valid position to unblock startup.
                        if chunk.windows(4).any(|window| window == b"\x1b[6n") {
                            let _ = writer_tx.send(b"\x1b[1;1R".to_vec()).await;
                        }
                        output.extend_from_slice(&chunk);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break exit_rx.await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                },
                result = &mut exit_rx => break result,
            }
        }
    })
    .await;
    let exit_code = match exit_code_result {
        Ok(Ok(code)) => code,
        Ok(Err(err)) => return Err(err.into()),
        Err(_) => {
            session.terminate();
            anyhow::bail!("timed out waiting for darwin_code CLI to exit");
        }
    };
    // Drain any output that raced with the exit notification.
    while let Ok(chunk) = output_rx.try_recv() {
        output.extend_from_slice(&chunk);
    }

    let output = String::from_utf8_lossy(&output);
    Ok(DarwinCodeCliOutput {
        exit_code,
        output: output.to_string(),
    })
}
