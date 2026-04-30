use crate::config::Config;
pub use darwin_code_rollout::ARCHIVED_SESSIONS_SUBDIR;
pub use darwin_code_rollout::Cursor;
pub use darwin_code_rollout::EventPersistenceMode;
pub use darwin_code_rollout::INTERACTIVE_SESSION_SOURCES;
pub use darwin_code_rollout::RolloutRecorder;
pub use darwin_code_rollout::RolloutRecorderParams;
pub use darwin_code_rollout::SESSIONS_SUBDIR;
pub use darwin_code_rollout::SessionMeta;
pub use darwin_code_rollout::SortDirection;
pub use darwin_code_rollout::ThreadItem;
pub use darwin_code_rollout::ThreadSortKey;
pub use darwin_code_rollout::ThreadsPage;
pub use darwin_code_rollout::append_thread_name;
pub use darwin_code_rollout::find_archived_thread_path_by_id_str;
#[deprecated(note = "use find_thread_path_by_id_str")]
pub use darwin_code_rollout::find_conversation_path_by_id_str;
pub use darwin_code_rollout::find_thread_meta_by_name_str;
pub use darwin_code_rollout::find_thread_name_by_id;
pub use darwin_code_rollout::find_thread_names_by_ids;
pub use darwin_code_rollout::find_thread_path_by_id_str;
pub use darwin_code_rollout::parse_cursor;
pub use darwin_code_rollout::read_head_for_summary;
pub use darwin_code_rollout::read_session_meta_line;
pub use darwin_code_rollout::rollout_date_parts;

impl darwin_code_rollout::RolloutConfigView for Config {
    fn codex_home(&self) -> &std::path::Path {
        self.darwin_code_home.as_path()
    }

    fn sqlite_home(&self) -> &std::path::Path {
        self.sqlite_home.as_path()
    }

    fn cwd(&self) -> &std::path::Path {
        self.cwd.as_path()
    }

    fn model_provider_id(&self) -> &str {
        self.model_provider_id.as_str()
    }

    fn generate_memories(&self) -> bool {
        self.memories.generate_memories
    }
}

pub(crate) mod list {
    pub use darwin_code_rollout::find_thread_path_by_id_str;
}

pub(crate) mod metadata {
    pub(crate) use darwin_code_rollout::builder_from_items;
}

pub(crate) mod policy {
    pub use darwin_code_rollout::EventPersistenceMode;
    pub use darwin_code_rollout::should_persist_response_item_for_memories;
}

pub(crate) mod recorder {
    pub use darwin_code_rollout::RolloutRecorder;
}

pub(crate) use crate::session_rollout_init_error::map_session_init_error;

pub(crate) mod truncation {
    pub(crate) use crate::thread_rollout_truncation::*;
}
