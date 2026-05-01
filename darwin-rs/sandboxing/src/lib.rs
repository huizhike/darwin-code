#[cfg(target_os = "linux")]
mod bwrap;
pub mod landlock;
mod manager;
pub mod policy_transforms;
#[cfg(target_os = "macos")]
pub mod seatbelt;

#[cfg(target_os = "linux")]
pub use bwrap::find_system_bwrap_in_path;
#[cfg(target_os = "linux")]
pub use bwrap::system_bwrap_warning;
pub use manager::SandboxCommand;
pub use manager::SandboxExecRequest;
pub use manager::SandboxManager;
pub use manager::SandboxTransformError;
pub use manager::SandboxTransformRequest;
pub use manager::SandboxType;
pub use manager::SandboxablePreference;
pub use manager::get_platform_sandbox;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkAccessRuntime;

impl NetworkAccessRuntime {
    pub fn apply_to_env(&self, _env: &mut std::collections::HashMap<String, String>) {}

    pub fn dangerously_allow_all_unix_sockets(&self) -> bool {
        false
    }

    pub fn allow_unix_sockets(&self) -> Vec<String> {
        Vec::new()
    }

    pub fn allow_local_binding(&self) -> bool {
        false
    }
}

use darwin_code_protocol::error::DarwinCodeErr;

#[cfg(not(target_os = "linux"))]
pub fn system_bwrap_warning(
    _sandbox_policy: &darwin_code_protocol::protocol::SandboxPolicy,
) -> Option<String> {
    None
}

impl From<SandboxTransformError> for DarwinCodeErr {
    fn from(err: SandboxTransformError) -> Self {
        match err {
            SandboxTransformError::MissingLinuxSandboxExecutable => {
                DarwinCodeErr::LandlockSandboxExecutableNotProvided
            }
            #[cfg(target_os = "linux")]
            SandboxTransformError::Wsl1UnsupportedForBubblewrap => {
                DarwinCodeErr::UnsupportedOperation(crate::bwrap::WSL1_BWRAP_WARNING.to_string())
            }
            #[cfg(not(target_os = "macos"))]
            SandboxTransformError::SeatbeltUnavailable => DarwinCodeErr::UnsupportedOperation(
                "seatbelt sandbox is only available on macOS".to_string(),
            ),
        }
    }
}
