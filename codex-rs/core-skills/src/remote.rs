use anyhow::Result;
use std::path::PathBuf;

// BYOK-only DarwinCode deliberately does not call remote skill APIs from the
// local engine.  The public data types remain so upstream call sites can keep their
// shape while remote skill listing/export is routed to an external gateway instead.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSkillScope {
    WorkspaceShared,
    AllShared,
    Personal,
    Example,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSkillProductSurface {
    Codex,
    Api,
    Atlas,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkillSummary {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSkillDownloadResult {
    pub id: String,
    pub path: PathBuf,
}

pub async fn list_remote_skills(
    _remote_base_url: String,
    _scope: RemoteSkillScope,
    _product_surface: RemoteSkillProductSurface,
    _enabled: Option<bool>,
) -> Result<Vec<RemoteSkillSummary>> {
    anyhow::bail!("remote skills are unavailable in BYOK-only DarwinCode")
}

pub async fn export_remote_skill(
    _remote_base_url: String,
    _codex_home: PathBuf,
    _skill_id: &str,
) -> Result<RemoteSkillDownloadResult> {
    anyhow::bail!("remote skill export is unavailable in BYOK-only DarwinCode")
}
