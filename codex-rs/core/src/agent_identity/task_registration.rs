use anyhow::Result;

use super::AgentIdentityManager;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RegisteredAgentTask {
    pub(crate) binding_id: String,
    pub(crate) provider_account_id: String,
    pub(crate) provider_user_id: Option<String>,
    pub(crate) agent_runtime_id: String,
    pub(crate) task_id: String,
    pub(crate) registered_at: String,
}

impl AgentIdentityManager {
    pub(crate) async fn register_task(&self) -> Result<Option<RegisteredAgentTask>> {
        Ok(None)
    }
}

impl RegisteredAgentTask {
    pub(crate) fn has_same_binding(&self, other: &RegisteredAgentTask) -> bool {
        self.binding_id == other.binding_id
            && self.provider_account_id == other.provider_account_id
            && self.provider_user_id == other.provider_user_id
    }
}
