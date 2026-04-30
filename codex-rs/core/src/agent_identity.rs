#![allow(dead_code)]

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

mod task_registration;

pub(crate) use task_registration::RegisteredAgentTask;

use crate::config::Config;
use darwin_code_protocol::protocol::SessionSource;

#[derive(Clone, Debug)]
pub(crate) struct AgentIdentityManager;

impl AgentIdentityManager {
    pub(crate) fn new(_config: &Config, _session_source: SessionSource) -> Self {
        Self
    }

    pub(crate) fn is_enabled(&self) -> bool {
        false
    }

    pub(crate) async fn ensure_registered_identity(&self) -> Result<Option<StoredAgentIdentity>> {
        Ok(None)
    }

    pub(crate) async fn task_matches_current_binding(&self, _task: &RegisteredAgentTask) -> bool {
        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StoredAgentIdentity {
    pub(crate) binding_id: String,
    pub(crate) provider_account_id: String,
    pub(crate) provider_user_id: Option<String>,
    pub(crate) agent_runtime_id: String,
    pub(crate) private_key_pkcs8_base64: String,
    pub(crate) public_key_ssh: String,
    pub(crate) registered_at: String,
    pub(crate) abom: AgentBillOfMaterials,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct AgentBillOfMaterials {
    pub(crate) agent_version: String,
    pub(crate) agent_harness_id: String,
    pub(crate) running_location: String,
}
