use crate::custom_ca::BuildCustomCaTransportError;
use crate::custom_ca::build_reqwest_client_with_custom_ca;
use darwin_code_terminal_detection::user_agent;
use http::Error as HttpError;
use http::HeaderMap;
use http::HeaderName;
use http::HeaderValue;
use opentelemetry::global;
use opentelemetry::propagation::Injector;
use reqwest::IntoUrl;
use reqwest::Method;
use reqwest::Response;
use serde::Serialize;
use std::fmt::Display;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::Duration;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Set this to add a suffix to the User-Agent string.
///
/// This is a process-global compatibility surface for clients that need to identify the current
/// local frontend (CLI, TUI, MCP server, etc.) without depending on the legacy login crate.
pub static USER_AGENT_SUFFIX: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
pub const DEFAULT_ORIGINATOR: &str = "darwin_code_cli_rs";
pub const DARWIN_CODE_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR: &str =
    "DARWIN_CODE_INTERNAL_ORIGINATOR_OVERRIDE";

#[derive(Debug, Clone)]
pub struct Originator {
    pub value: String,
    pub header_value: HeaderValue,
}

#[derive(Debug)]
pub enum SetOriginatorError {
    InvalidHeaderValue,
    AlreadyInitialized,
}

static ORIGINATOR: LazyLock<RwLock<Option<Originator>>> = LazyLock::new(|| RwLock::new(None));

fn get_originator_value(provided: Option<String>) -> Originator {
    let value = std::env::var(DARWIN_CODE_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR)
        .ok()
        .or(provided)
        .unwrap_or(DEFAULT_ORIGINATOR.to_string());

    match HeaderValue::from_str(&value) {
        Ok(header_value) => Originator {
            value,
            header_value,
        },
        Err(e) => {
            tracing::error!("Unable to turn originator override {value} into header value: {e}");
            Originator {
                value: DEFAULT_ORIGINATOR.to_string(),
                header_value: HeaderValue::from_static(DEFAULT_ORIGINATOR),
            }
        }
    }
}

pub fn set_default_originator(value: String) -> Result<(), SetOriginatorError> {
    if HeaderValue::from_str(&value).is_err() {
        return Err(SetOriginatorError::InvalidHeaderValue);
    }
    let originator = get_originator_value(Some(value));
    let Ok(mut guard) = ORIGINATOR.write() else {
        return Err(SetOriginatorError::AlreadyInitialized);
    };
    if guard.is_some() {
        return Err(SetOriginatorError::AlreadyInitialized);
    }
    *guard = Some(originator);
    Ok(())
}

/// BYOK-only compatibility hook; residency routing is not handled inside DarwinCode.
pub fn set_default_client_residency_requirement<T>(_enforce_residency: Option<T>) {}

pub fn originator() -> Originator {
    if let Ok(guard) = ORIGINATOR.read()
        && let Some(originator) = guard.as_ref()
    {
        return originator.clone();
    }

    if std::env::var(DARWIN_CODE_INTERNAL_ORIGINATOR_OVERRIDE_ENV_VAR).is_ok() {
        let originator = get_originator_value(/*provided*/ None);
        if let Ok(mut guard) = ORIGINATOR.write() {
            match guard.as_ref() {
                Some(originator) => return originator.clone(),
                None => *guard = Some(originator.clone()),
            }
        }
        return originator;
    }

    get_originator_value(/*provided*/ None)
}

pub fn is_first_party_originator(originator_value: &str) -> bool {
    originator_value == DEFAULT_ORIGINATOR
        || originator_value == "darwin-code-tui"
        || originator_value == "darwin_code_vscode"
        || originator_value.starts_with("DarwinCode ")
}

pub fn is_first_party_chat_originator(originator_value: &str) -> bool {
    originator_value == "darwin_code_atlas"
}

pub fn get_darwin_code_user_agent() -> String {
    let build_version = env!("CARGO_PKG_VERSION");
    let os_info = os_info::get();
    let originator = originator();
    let prefix = format!(
        "{}/{build_version} ({} {}; {}) {}",
        originator.value.as_str(),
        os_info.os_type(),
        os_info.version(),
        os_info.architecture().unwrap_or("unknown"),
        user_agent()
    );
    let suffix = USER_AGENT_SUFFIX
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let suffix = suffix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(String::new, |value| format!(" ({value})"));

    let candidate = format!("{prefix}{suffix}");
    sanitize_user_agent(candidate, &prefix)
}

fn sanitize_user_agent(candidate: String, fallback: &str) -> String {
    if HeaderValue::from_str(candidate.as_str()).is_ok() {
        return candidate;
    }

    let sanitized: String = candidate
        .chars()
        .map(|ch| if matches!(ch, ' '..='~') { ch } else { '_' })
        .collect();
    if !sanitized.is_empty() && HeaderValue::from_str(sanitized.as_str()).is_ok() {
        tracing::warn!(
            "Sanitized DarwinCode user agent because provided suffix contained invalid header characters"
        );
        sanitized
    } else if HeaderValue::from_str(fallback).is_ok() {
        tracing::warn!(
            "Falling back to base DarwinCode user agent because provided suffix could not be sanitized"
        );
        fallback.to_string()
    } else {
        tracing::warn!(
            "Falling back to default DarwinCode originator because base user agent string is invalid"
        );
        originator().value
    }
}

pub fn create_client() -> DarwinCodeHttpClient {
    DarwinCodeHttpClient::new(build_reqwest_client())
}

pub fn build_reqwest_client() -> reqwest::Client {
    try_build_reqwest_client().unwrap_or_else(|error| {
        tracing::warn!(error = %error, "failed to build default reqwest client");
        reqwest::Client::new()
    })
}

pub fn try_build_reqwest_client() -> Result<reqwest::Client, BuildCustomCaTransportError> {
    let ua = get_darwin_code_user_agent();

    let mut builder = reqwest::Client::builder()
        .user_agent(ua)
        .default_headers(default_headers());
    if is_sandboxed() {
        builder = builder.no_proxy();
    }

    build_reqwest_client_with_custom_ca(builder)
}

pub fn default_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("originator", originator().header_value);
    headers
}

fn is_sandboxed() -> bool {
    std::env::var("DARWIN_CODE_SANDBOX").as_deref() == Ok("seatbelt")
}

#[derive(Clone, Debug)]
pub struct DarwinCodeHttpClient {
    inner: reqwest::Client,
}

impl DarwinCodeHttpClient {
    pub fn new(inner: reqwest::Client) -> Self {
        Self { inner }
    }

    pub fn get<U>(&self, url: U) -> DarwinCodeRequestBuilder
    where
        U: IntoUrl,
    {
        self.request(Method::GET, url)
    }

    pub fn post<U>(&self, url: U) -> DarwinCodeRequestBuilder
    where
        U: IntoUrl,
    {
        self.request(Method::POST, url)
    }

    pub fn request<U>(&self, method: Method, url: U) -> DarwinCodeRequestBuilder
    where
        U: IntoUrl,
    {
        let url_str = url.as_str().to_string();
        DarwinCodeRequestBuilder::new(self.inner.request(method.clone(), url), method, url_str)
    }
}

#[must_use = "requests are not sent unless `send` is awaited"]
#[derive(Debug)]
pub struct DarwinCodeRequestBuilder {
    builder: reqwest::RequestBuilder,
    method: Method,
    url: String,
}

impl DarwinCodeRequestBuilder {
    fn new(builder: reqwest::RequestBuilder, method: Method, url: String) -> Self {
        Self {
            builder,
            method,
            url,
        }
    }

    fn map(self, f: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder) -> Self {
        Self {
            builder: f(self.builder),
            method: self.method,
            url: self.url,
        }
    }

    pub fn headers(self, headers: HeaderMap) -> Self {
        self.map(|builder| builder.headers(headers))
    }

    pub fn header<K, V>(self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<HttpError>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<HttpError>,
    {
        self.map(|builder| builder.header(key, value))
    }

    pub fn bearer_auth<T>(self, token: T) -> Self
    where
        T: Display,
    {
        self.map(|builder| builder.bearer_auth(token))
    }

    pub fn timeout(self, timeout: Duration) -> Self {
        self.map(|builder| builder.timeout(timeout))
    }

    pub fn json<T>(self, value: &T) -> Self
    where
        T: ?Sized + Serialize,
    {
        self.map(|builder| builder.json(value))
    }

    pub fn body<B>(self, body: B) -> Self
    where
        B: Into<reqwest::Body>,
    {
        self.map(|builder| builder.body(body))
    }

    pub async fn send(self) -> Result<Response, reqwest::Error> {
        let headers = trace_headers();

        match self.builder.headers(headers).send().await {
            Ok(response) => {
                tracing::debug!(
                    method = %self.method,
                    url = %self.url,
                    status = %response.status(),
                    headers = ?response.headers(),
                    version = ?response.version(),
                    "Request completed"
                );

                Ok(response)
            }
            Err(error) => {
                let status = error.status();
                tracing::debug!(
                    method = %self.method,
                    url = %self.url,
                    status = status.map(|s| s.as_u16()),
                    error = %error,
                    "Request failed"
                );
                Err(error)
            }
        }
    }
}

struct HeaderMapInjector<'a>(&'a mut HeaderMap);

impl<'a> Injector for HeaderMapInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, val);
        }
    }
}

fn trace_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    global::get_text_map_propagator(|prop| {
        prop.inject_context(
            &Span::current().context(),
            &mut HeaderMapInjector(&mut headers),
        );
    });
    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::propagation::Extractor;
    use opentelemetry::propagation::TextMapPropagator;
    use opentelemetry::trace::TraceContextExt;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::propagation::TraceContextPropagator;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing::trace_span;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    #[test]
    fn default_reqwest_client_inputs_include_originator_and_user_agent() {
        assert_eq!(
            default_headers()
                .get("originator")
                .and_then(|value| value.to_str().ok()),
            Some(originator().value.as_str())
        );
        assert!(
            get_darwin_code_user_agent().starts_with(originator().value.as_str()),
            "user agent should carry the originator prefix"
        );
    }

    #[test]
    fn inject_trace_headers_uses_current_span_context() {
        global::set_text_map_propagator(TraceContextPropagator::new());

        let provider = SdkTracerProvider::builder().build();
        let tracer = provider.tracer("test-tracer");
        let subscriber =
            tracing_subscriber::registry().with(tracing_opentelemetry::layer().with_tracer(tracer));
        let _guard = subscriber.set_default();

        let span = trace_span!("client_request");
        let _entered = span.enter();
        let span_context = span.context().span().span_context().clone();

        let headers = trace_headers();

        let extractor = HeaderMapExtractor(&headers);
        let extracted = TraceContextPropagator::new().extract(&extractor);
        let extracted_span = extracted.span();
        let extracted_context = extracted_span.span_context();

        assert!(extracted_context.is_valid());
        assert_eq!(extracted_context.trace_id(), span_context.trace_id());
        assert_eq!(extracted_context.span_id(), span_context.span_id());
    }

    struct HeaderMapExtractor<'a>(&'a HeaderMap);

    impl<'a> Extractor for HeaderMapExtractor<'a> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).and_then(|value| value.to_str().ok())
        }

        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(HeaderName::as_str).collect()
        }
    }
}
