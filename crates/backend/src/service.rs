use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;

use crate::{
    BackendError, ChangeRecord, CreateEntityParams, DeleteEntityParams, MutationResult,
    ReadEntityParams, ReadEntityResult, SubscribeChangesParams, UpdateEntityParams,
};

pub type ChangeStream =
    Pin<Box<dyn Stream<Item = Result<ChangeRecord, BackendError>> + Send + 'static>>;

#[async_trait]
pub trait BackendService: Send + Sync {
    async fn create(&self, params: CreateEntityParams) -> Result<MutationResult, BackendError>;

    async fn read(&self, params: ReadEntityParams) -> Result<ReadEntityResult, BackendError>;

    async fn update(&self, params: UpdateEntityParams) -> Result<MutationResult, BackendError>;

    async fn delete(&self, params: DeleteEntityParams) -> Result<MutationResult, BackendError>;

    async fn subscribe(&self, params: SubscribeChangesParams)
    -> Result<ChangeStream, BackendError>;
}
