mod custom_ca;
mod default_client;
mod error;
mod request;
mod retry;
mod sse;
mod transport;

pub use crate::custom_ca::BuildCustomCaTransportError;
/// Test-only subprocess hook for custom CA coverage.
///
/// This stays public only so the `custom_ca_probe` binary target can reuse the shared helper. It
/// is hidden from normal docs because ordinary callers should use
/// [`build_reqwest_client_with_custom_ca`] instead.
#[doc(hidden)]
pub use crate::custom_ca::build_reqwest_client_for_subprocess_tests;
pub use crate::custom_ca::build_reqwest_client_with_custom_ca;
pub use crate::custom_ca::maybe_build_rustls_client_config_with_custom_ca;
pub use crate::default_client::DARWIN_CODE_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR;
pub use crate::default_client::DEFAULT_ORIGINATOR;
pub use crate::default_client::DarwinCodeHttpClient;
pub use crate::default_client::DarwinCodeRequestBuilder;
pub use crate::default_client::Originator;
pub use crate::default_client::SetOriginatorError;
pub use crate::default_client::USER_AGENT_SUFFIX;
pub use crate::default_client::build_reqwest_client;
pub use crate::default_client::create_client;
pub use crate::default_client::default_headers;
pub use crate::default_client::get_darwin_code_user_agent;
pub use crate::default_client::is_first_party_chat_originator;
pub use crate::default_client::is_first_party_originator;
pub use crate::default_client::originator;
pub use crate::default_client::set_default_client_residency_requirement;
pub use crate::default_client::set_default_originator;
pub use crate::default_client::try_build_reqwest_client;
pub use crate::error::StreamError;
pub use crate::error::TransportError;
pub use crate::request::Request;
pub use crate::request::RequestBody;
pub use crate::request::RequestCompression;
pub use crate::request::Response;
pub use crate::retry::RetryOn;
pub use crate::retry::RetryPolicy;
pub use crate::retry::backoff;
pub use crate::retry::run_with_retry;
pub use crate::sse::sse_stream;
pub use crate::transport::ByteStream;
pub use crate::transport::HttpTransport;
pub use crate::transport::ReqwestTransport;
pub use crate::transport::StreamResponse;
