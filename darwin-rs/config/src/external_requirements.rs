use crate::config_requirements::ConfigRequirementsToml;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use futures::future::Shared;
use std::fmt;
use std::future::Future;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalRequirementsLoadErrorCode {
    Auth,
    Timeout,
    Parse,
    RequestFailed,
    Internal,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{message}")]
pub struct ExternalRequirementsLoadError {
    code: ExternalRequirementsLoadErrorCode,
    message: String,
    status_code: Option<u16>,
}

impl ExternalRequirementsLoadError {
    pub fn new(
        code: ExternalRequirementsLoadErrorCode,
        status_code: Option<u16>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            status_code,
        }
    }

    pub fn code(&self) -> ExternalRequirementsLoadErrorCode {
        self.code
    }

    pub fn status_code(&self) -> Option<u16> {
        self.status_code
    }
}

#[derive(Clone)]
pub struct ExternalRequirementsLoader {
    fut: Shared<
        BoxFuture<'static, Result<Option<ConfigRequirementsToml>, ExternalRequirementsLoadError>>,
    >,
}

impl ExternalRequirementsLoader {
    pub fn new<F>(fut: F) -> Self
    where
        F: Future<Output = Result<Option<ConfigRequirementsToml>, ExternalRequirementsLoadError>>
            + Send
            + 'static,
    {
        Self {
            fut: fut.boxed().shared(),
        }
    }

    pub async fn get(
        &self,
    ) -> Result<Option<ConfigRequirementsToml>, ExternalRequirementsLoadError> {
        self.fut.clone().await
    }
}

impl fmt::Debug for ExternalRequirementsLoader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalRequirementsLoader").finish()
    }
}

impl Default for ExternalRequirementsLoader {
    fn default() -> Self {
        Self::new(async { Ok(None) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn shared_future_runs_once() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let loader = ExternalRequirementsLoader::new(async move {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ConfigRequirementsToml::default()))
        });

        let (first, second) = tokio::join!(loader.get(), loader.get());
        assert_eq!(first, second);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
