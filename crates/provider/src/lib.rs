use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ProviderError {
    message: String,
}

impl ProviderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderRecovery {
    pub generation: u64,
    pub recovered_after_unclean_shutdown: bool,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn kind(&self) -> &'static str;

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError>;

    async fn health_check(&self) -> Result<(), ProviderError>;

    async fn checkpoint(&self) -> Result<(), ProviderError>;

    async fn shutdown(&self) -> Result<(), ProviderError>;
}
