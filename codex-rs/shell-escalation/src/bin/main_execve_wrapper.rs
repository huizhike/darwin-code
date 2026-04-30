#[cfg(not(unix))]
fn main() {
    eprintln!("codex-execve-wrapper is only implemented for UNIX");
    std::process::exit(1);
}

#[cfg(unix)]
pub use darwin_code_shell_escalation::main_execve_wrapper as main;
