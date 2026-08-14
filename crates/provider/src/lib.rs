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

#[async_trait]
pub trait Provider: Send + Sync {
    fn kind(&self) -> &'static str;

    async fn health_check(&self) -> Result<(), ProviderError>;
}
