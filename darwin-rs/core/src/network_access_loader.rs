use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct NetworkAccessState;

#[derive(Debug, Default)]
pub struct MtimeConfigReloader;

pub async fn build_network_access_state() -> Result<NetworkAccessState> {
    Ok(NetworkAccessState)
}

pub async fn build_network_access_state_and_reloader()
-> Result<(NetworkAccessState, MtimeConfigReloader)> {
    Ok((NetworkAccessState, MtimeConfigReloader))
}
