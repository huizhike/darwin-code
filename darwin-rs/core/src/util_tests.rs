use std::path::Path;
use std::path::PathBuf;

use super::normalize_thread_name;
use super::resolve_path;

#[test]
fn normalize_thread_name_trims_and_rejects_empty_names() {
    assert_eq!(
        normalize_thread_name("  review plan  "),
        Some("review plan".to_string())
    );
    assert_eq!(normalize_thread_name("   "), None);
}

#[test]
fn resolve_path_keeps_absolute_paths_and_joins_relative_paths() {
    let base = Path::new("/workspace/project");

    assert_eq!(
        resolve_path(base, &PathBuf::from("docs/config.md")),
        PathBuf::from("/workspace/project/docs/config.md")
    );
    assert_eq!(
        resolve_path(base, &PathBuf::from("/tmp/config.md")),
        PathBuf::from("/tmp/config.md")
    );
}
