use std::collections::HashMap;

use darwin_code_config::McpServerConfig;
use darwin_code_config::McpServerTransportConfig;
use darwin_code_protocol::protocol::McpAuthStatus;
use darwin_code_rmcp_client::determine_streamable_http_auth_status;
use futures::future::join_all;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct McpAuthStatusEntry {
    pub config: McpServerConfig,
    pub auth_status: McpAuthStatus,
}

pub async fn compute_auth_statuses<'a, I>(servers: I) -> HashMap<String, McpAuthStatusEntry>
where
    I: IntoIterator<Item = (&'a String, &'a McpServerConfig)>,
{
    let futures = servers.into_iter().map(|(name, config)| {
        let name = name.clone();
        let config = config.clone();
        async move {
            let auth_status = match compute_auth_status(&config).await {
                Ok(status) => status,
                Err(error) => {
                    warn!("failed to determine auth status for MCP server `{name}`: {error:?}");
                    McpAuthStatus::Unsupported
                }
            };
            let entry = McpAuthStatusEntry {
                config,
                auth_status,
            };
            (name, entry)
        }
    });

    join_all(futures).await.into_iter().collect()
}

async fn compute_auth_status(config: &McpServerConfig) -> anyhow::Result<McpAuthStatus> {
    if !config.enabled {
        return Ok(McpAuthStatus::Unsupported);
    }

    match &config.transport {
        McpServerTransportConfig::Stdio { .. } => Ok(McpAuthStatus::Unsupported),
        McpServerTransportConfig::StreamableHttp {
            bearer_token_env_var,
            http_headers,
            env_http_headers,
            ..
        } => {
            determine_streamable_http_auth_status(
                bearer_token_env_var.as_deref(),
                http_headers.clone(),
                env_http_headers.clone(),
            )
            .await
        }
    }
}
