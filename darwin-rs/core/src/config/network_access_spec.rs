use crate::config_loader::NetworkConstraints;
use darwin_code_config::permissions_toml::NetworkAccessConfig;
use darwin_code_protocol::protocol::SandboxPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAccessSpec {
    config: NetworkAccessConfig,
    _requirements: Option<NetworkConstraints>,
}

#[derive(Debug, Clone, Default)]
pub struct StartedNetworkAccess;

impl NetworkAccessSpec {
    pub(crate) fn enabled(&self) -> bool {
        false
    }

    pub fn proxy_host_and_port(&self) -> String {
        self.config.network.proxy_url.clone()
    }

    pub fn socks_enabled(&self) -> bool {
        false
    }

    pub(crate) fn from_config_and_constraints(
        config: NetworkAccessConfig,
        requirements: Option<NetworkConstraints>,
        _sandbox_policy: &SandboxPolicy,
    ) -> std::io::Result<Self> {
        Ok(Self {
            config,
            _requirements: requirements,
        })
    }
}
