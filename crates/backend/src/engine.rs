use std::sync::Arc;

use async_trait::async_trait;
use patchouli_provider::{Provider, ProviderError, ProviderRecovery};
use thiserror::Error;

use crate::{
    BackendConfig, BackendError, BackendErrorReason, BackendService, ChangeStream,
    CreateEntityParams, DeleteEntityParams, MutationResult, ReadEntityParams, ReadEntityResult,
    SubscribeChangesParams, UpdateEntityParams,
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("database provider is not ready: {0}")]
    Provider(#[from] ProviderError),
}

pub struct BackendEngine {
    config: BackendConfig,
    provider: Arc<dyn Provider>,
    recovery: ProviderRecovery,
}

impl BackendEngine {
    pub async fn start(
        config: BackendConfig,
        provider: Arc<dyn Provider>,
    ) -> Result<Self, EngineError> {
        let recovery = provider.initialize().await?;
        if let Err(health_error) = provider.health_check().await {
            return match provider.shutdown().await {
                Ok(()) => Err(EngineError::Provider(health_error)),
                Err(shutdown_error) => Err(EngineError::Provider(ProviderError::new(format!(
                    "{health_error}; provider cleanup also failed: {shutdown_error}"
                )))),
            };
        }
        Ok(Self {
            config,
            provider,
            recovery,
        })
    }

    pub fn config(&self) -> &BackendConfig {
        &self.config
    }

    pub fn provider_kind(&self) -> &'static str {
        self.provider.kind()
    }

    pub fn recovery(&self) -> ProviderRecovery {
        self.recovery
    }

    pub async fn checkpoint(&self) -> Result<(), EngineError> {
        self.provider.checkpoint().await?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), EngineError> {
        self.provider.shutdown().await?;
        Ok(())
    }
}

#[async_trait]
impl BackendService for BackendEngine {
    async fn create(&self, _params: CreateEntityParams) -> Result<MutationResult, BackendError> {
        Err(not_implemented("entity create"))
    }

    async fn read(&self, _params: ReadEntityParams) -> Result<ReadEntityResult, BackendError> {
        Err(not_implemented("entity read"))
    }

    async fn update(&self, _params: UpdateEntityParams) -> Result<MutationResult, BackendError> {
        Err(not_implemented("entity update"))
    }

    async fn delete(&self, _params: DeleteEntityParams) -> Result<MutationResult, BackendError> {
        Err(not_implemented("entity delete"))
    }

    async fn subscribe(
        &self,
        _params: SubscribeChangesParams,
    ) -> Result<ChangeStream, BackendError> {
        Err(not_implemented("change subscription"))
    }
}

fn not_implemented(operation: &str) -> BackendError {
    BackendError::new(
        BackendErrorReason::UnsupportedCapability,
        format!("{operation} is not implemented by the backend engine"),
    )
}
