use std::path::PathBuf;

use darwin_code_utils_absolute_path::AbsolutePathBuf;

/// Runtime paths needed by exec-server child processes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecServerRuntimePaths {
    /// Stable path to the DarwinCode executable used to launch hidden helper modes.
    pub darwin_code_self_exe: AbsolutePathBuf,
    /// Path to the Linux sandbox helper alias used when the platform sandbox
    /// needs to re-enter DarwinCode by argv0.
    pub darwin_code_linux_sandbox_exe: Option<AbsolutePathBuf>,
}

impl ExecServerRuntimePaths {
    pub fn from_optional_paths(
        darwin_code_self_exe: Option<PathBuf>,
        darwin_code_linux_sandbox_exe: Option<PathBuf>,
    ) -> std::io::Result<Self> {
        let darwin_code_self_exe = darwin_code_self_exe.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "DarwinCode executable path is not configured",
            )
        })?;
        Self::new(darwin_code_self_exe, darwin_code_linux_sandbox_exe)
    }

    pub fn new(
        darwin_code_self_exe: PathBuf,
        darwin_code_linux_sandbox_exe: Option<PathBuf>,
    ) -> std::io::Result<Self> {
        Ok(Self {
            darwin_code_self_exe: absolute_path(darwin_code_self_exe)?,
            darwin_code_linux_sandbox_exe: darwin_code_linux_sandbox_exe
                .map(absolute_path)
                .transpose()?,
        })
    }
}

fn absolute_path(path: PathBuf) -> std::io::Result<AbsolutePathBuf> {
    AbsolutePathBuf::from_absolute_path(path.as_path())
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))
}
