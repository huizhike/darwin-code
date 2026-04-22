use std::sync::Arc;

use darwin_code_api::SharedAuthProvider;
use darwin_code_login::AuthManager;
use darwin_code_login::DarwinCodeAuth;
use darwin_code_model_provider_info::ModelProviderInfo;

use crate::bearer_auth_provider::BearerAuthProvider;

/// Returns the provider-scoped auth manager when this provider uses command-backed auth.
///
/// Providers without custom auth continue using the caller-supplied base manager, when present.
pub(crate) fn auth_manager_for_provider(
    auth_manager: Option<Arc<AuthManager>>,
    provider: &ModelProviderInfo,
) -> Option<Arc<AuthManager>> {
    match provider.auth.clone() {
        Some(config) => Some(AuthManager::external_bearer_only(config)),
        None => auth_manager,
    }
}

fn bearer_auth_provider_from_auth(
    auth: Option<&DarwinCodeAuth>,
    provider: &ModelProviderInfo,
) -> darwin_code_protocol::error::Result<BearerAuthProvider> {
    if let Some(api_key) = provider.api_key()? {
        return Ok(BearerAuthProvider {
            token: Some(api_key),
            account_id: None,
            is_fedramp_account: false,
        });
    }

    if let Some(token) = provider.experimental_bearer_token.clone() {
        return Ok(BearerAuthProvider {
            token: Some(token),
            account_id: None,
            is_fedramp_account: false,
        });
    }

    if let Some(auth) = auth {
        let token = auth.get_token()?;
        Ok(BearerAuthProvider {
            token: Some(token),
            account_id: auth.get_account_id(),
            is_fedramp_account: auth.is_fedramp_account(),
        })
    } else {
        Ok(BearerAuthProvider {
            token: None,
            account_id: None,
            is_fedramp_account: false,
        })
    }
}

pub(crate) fn resolve_provider_auth(
    auth: Option<&DarwinCodeAuth>,
    provider: &ModelProviderInfo,
) -> darwin_code_protocol::error::Result<SharedAuthProvider> {
    Ok(Arc::new(bearer_auth_provider_from_auth(auth, provider)?))
}
