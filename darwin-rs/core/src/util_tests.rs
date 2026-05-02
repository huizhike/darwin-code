use std::path::Path;
use std::path::PathBuf;

use darwin_code_protocol::ThreadId;

use super::normalize_thread_name;
use super::resolve_path;
use super::resume_command;

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

#[test]
fn resume_command_prefers_name_over_id() {
    let thread_id = ThreadId::from_string("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let command = resume_command(Some("my-thread"), Some(thread_id));
    assert_eq!(command, Some("darwin-code resume my-thread".to_string()));
}

#[test]
fn resume_command_with_only_id() {
    let thread_id = ThreadId::from_string("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let command = resume_command(/*thread_name*/ None, Some(thread_id));
    assert_eq!(
        command,
        Some("darwin-code resume 123e4567-e89b-12d3-a456-426614174000".to_string())
    );
}

#[test]
fn resume_command_with_no_name_or_id() {
    let command = resume_command(/*thread_name*/ None, /*thread_id*/ None);
    assert_eq!(command, None);
}

#[test]
fn resume_command_quotes_thread_name_when_needed() {
    let command = resume_command(Some("-starts-with-dash"), /*thread_id*/ None);
    assert_eq!(
        command,
        Some("darwin-code resume -- -starts-with-dash".to_string())
    );

    let command = resume_command(Some("two words"), /*thread_id*/ None);
    assert_eq!(command, Some("darwin-code resume 'two words'".to_string()));

    let command = resume_command(Some("quote'case"), /*thread_id*/ None);
    assert_eq!(
        command,
        Some("darwin-code resume \"quote'case\"".to_string())
    );
}
