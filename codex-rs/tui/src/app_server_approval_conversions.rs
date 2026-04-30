use darwin_code_app_server_protocol::AdditionalFileSystemPermissions;
use darwin_code_app_server_protocol::AdditionalNetworkPermissions;
use darwin_code_app_server_protocol::GrantedPermissionProfile;
use darwin_code_app_server_protocol::NetworkApprovalContext as AppServerNetworkApprovalContext;
use darwin_code_protocol::protocol::NetworkApprovalContext;
use darwin_code_protocol::protocol::NetworkApprovalProtocol;
use darwin_code_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;

pub(crate) fn network_approval_context_to_core(
    value: AppServerNetworkApprovalContext,
) -> NetworkApprovalContext {
    NetworkApprovalContext {
        host: value.host,
        protocol: match value.protocol {
            darwin_code_app_server_protocol::NetworkApprovalProtocol::Http => {
                NetworkApprovalProtocol::Http
            }
            darwin_code_app_server_protocol::NetworkApprovalProtocol::Https => {
                NetworkApprovalProtocol::Https
            }
            darwin_code_app_server_protocol::NetworkApprovalProtocol::Socks5Tcp => {
                NetworkApprovalProtocol::Socks5Tcp
            }
            darwin_code_app_server_protocol::NetworkApprovalProtocol::Socks5Udp => {
                NetworkApprovalProtocol::Socks5Udp
            }
        },
    }
}

pub(crate) fn granted_permission_profile_from_request(
    value: CoreRequestPermissionProfile,
) -> GrantedPermissionProfile {
    GrantedPermissionProfile {
        network: value.network.map(|network| AdditionalNetworkPermissions {
            enabled: network.enabled,
        }),
        file_system: value
            .file_system
            .map(|file_system| AdditionalFileSystemPermissions {
                read: file_system.read,
                write: file_system.write,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::granted_permission_profile_from_request;
    use super::network_approval_context_to_core;
    use darwin_code_protocol::models::FileSystemPermissions;
    use darwin_code_protocol::models::NetworkPermissions;
    use darwin_code_protocol::protocol::NetworkApprovalContext;
    use darwin_code_protocol::protocol::NetworkApprovalProtocol;
    use darwin_code_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;
    use darwin_code_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    fn absolute_path(path: &str) -> AbsolutePathBuf {
        AbsolutePathBuf::try_from(PathBuf::from(path)).expect("path must be absolute")
    }

    #[test]
    fn converts_app_server_network_approval_context_to_core() {
        assert_eq!(
            network_approval_context_to_core(
                darwin_code_app_server_protocol::NetworkApprovalContext {
                    host: "example.com".to_string(),
                    protocol: darwin_code_app_server_protocol::NetworkApprovalProtocol::Socks5Tcp,
                }
            ),
            NetworkApprovalContext {
                host: "example.com".to_string(),
                protocol: NetworkApprovalProtocol::Socks5Tcp,
            }
        );
    }

    #[test]
    fn converts_request_permissions_into_granted_permissions() {
        assert_eq!(
            granted_permission_profile_from_request(CoreRequestPermissionProfile {
                network: Some(NetworkPermissions {
                    enabled: Some(true),
                }),
                file_system: Some(FileSystemPermissions {
                    read: Some(vec![absolute_path("/tmp/read-only")]),
                    write: Some(vec![absolute_path("/tmp/write")]),
                }),
            }),
            darwin_code_app_server_protocol::GrantedPermissionProfile {
                network: Some(
                    darwin_code_app_server_protocol::AdditionalNetworkPermissions {
                        enabled: Some(true),
                    }
                ),
                file_system: Some(
                    darwin_code_app_server_protocol::AdditionalFileSystemPermissions {
                        read: Some(vec![absolute_path("/tmp/read-only")]),
                        write: Some(vec![absolute_path("/tmp/write")]),
                    }
                ),
            }
        );
    }
}
