use std::sync::Arc;

use async_trait::async_trait;
use patchouli_provider::{Provider, ProviderError};
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
}

impl BackendEngine {
    pub async fn start(
        config: BackendConfig,
        provider: Arc<dyn Provider>,
    ) -> Result<Self, EngineError> {
        provider.health_check().await?;
        Ok(Self { config, provider })
    }

    pub fn config(&self) -> &BackendConfig {
        &self.config
    }

    pub fn provider_kind(&self) -> &'static str {
        self.provider.kind()
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
