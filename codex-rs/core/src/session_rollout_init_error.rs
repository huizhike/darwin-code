use std::io::ErrorKind;
use std::path::Path;

use crate::rollout::SESSIONS_SUBDIR;
use darwin_code_protocol::error::DarwinCodeErr;

pub(crate) fn map_session_init_error(err: &anyhow::Error, darwin_code_home: &Path) -> DarwinCodeErr {
    if let Some(mapped) = err
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .find_map(|io_err| map_rollout_io_error(io_err, darwin_code_home))
    {
        return mapped;
    }

    DarwinCodeErr::Fatal(format!("Failed to initialize session: {err:#}"))
}

fn map_rollout_io_error(io_err: &std::io::Error, darwin_code_home: &Path) -> Option<DarwinCodeErr> {
    let sessions_dir = darwin_code_home.join(SESSIONS_SUBDIR);
    let hint = match io_err.kind() {
        ErrorKind::PermissionDenied => format!(
            "Darwin-Code cannot access session files at {} (permission denied). If sessions were created using sudo, fix ownership: sudo chown -R $(whoami) {}",
            sessions_dir.display(),
            darwin_code_home.display()
        ),
        ErrorKind::NotFound => format!(
            "Session storage missing at {}. Create the directory or choose a different Darwin-Code home.",
            sessions_dir.display()
        ),
        ErrorKind::AlreadyExists => format!(
            "Session storage path {} is blocked by an existing file. Remove or rename it so Darwin-Code can create sessions.",
            sessions_dir.display()
        ),
        ErrorKind::InvalidData | ErrorKind::InvalidInput => format!(
            "Session data under {} looks corrupt or unreadable. Clearing the sessions directory may help (this will remove saved threads).",
            sessions_dir.display()
        ),
        ErrorKind::IsADirectory | ErrorKind::NotADirectory => format!(
            "Session storage path {} has an unexpected type. Ensure it is a directory Darwin-Code can use for session files.",
            sessions_dir.display()
        ),
        _ => return None,
    };

    Some(DarwinCodeErr::Fatal(format!(
        "{hint} (underlying error: {io_err})"
    )))
}
