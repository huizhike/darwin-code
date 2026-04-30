pub(crate) use darwin_code_skills::install_system_skills;
pub(crate) use darwin_code_skills::system_cache_root_dir;

use darwin_code_utils_absolute_path::AbsolutePathBuf;

pub(crate) fn uninstall_system_skills(codex_home: &AbsolutePathBuf) {
    let _ = std::fs::remove_dir_all(system_cache_root_dir(codex_home));
}
