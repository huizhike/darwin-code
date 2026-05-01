use darwin_code_protocol::config_types::WindowsSandboxLevel;
use darwin_code_protocol::protocol::SandboxPolicy;
use darwin_code_sandboxing::SandboxType;
use darwin_code_sandboxing::get_platform_sandbox;

pub(crate) fn sandbox_tag(
    policy: &SandboxPolicy,
    windows_sandbox_level: WindowsSandboxLevel,
) -> &'static str {
    if matches!(policy, SandboxPolicy::DangerFullAccess) {
        return "none";
    }
    if matches!(policy, SandboxPolicy::ExternalSandbox { .. }) {
        return "external";
    }
    if cfg!(target_os = "windows") && matches!(windows_sandbox_level, WindowsSandboxLevel::Elevated)
    {
        return "windows_elevated";
    }

    get_platform_sandbox(windows_sandbox_level != WindowsSandboxLevel::Disabled)
        .map(SandboxType::as_metric_tag)
        .unwrap_or("none")
}

#[cfg(test)]
#[path = "sandbox_tags_tests.rs"]
mod tests;
