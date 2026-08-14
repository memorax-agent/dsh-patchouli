use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;

use crate::{
    BackendError, ChangeRecord, CreateEntityParams, DeleteEntityParams, Meta, MutationResult,
    ReadEntityParams, ReadEntityResult, RetrieveEntitiesParams, RetrieveEntitiesResult,
    SubscribeChangesParams, UpdateEntityParams,
};

pub struct PublishedChange {
    pub meta: Meta,
    pub change: ChangeRecord,
}

pub type ChangeStream =
    Pin<Box<dyn Stream<Item = Result<PublishedChange, BackendError>> + Send + 'static>>;

pub struct ChangeSubscription {
    pub cursor: String,
    pub stream: ChangeStream,
}

#[async_trait]
pub trait BackendService: Send + Sync {
    async fn create(&self, params: CreateEntityParams) -> Result<MutationResult, BackendError>;

    async fn read(&self, params: ReadEntityParams) -> Result<ReadEntityResult, BackendError>;

    async fn retrieve(
        &self,
        params: RetrieveEntitiesParams,
    ) -> Result<RetrieveEntitiesResult, BackendError>;

    async fn update(&self, params: UpdateEntityParams) -> Result<MutationResult, BackendError>;

    async fn delete(&self, params: DeleteEntityParams) -> Result<MutationResult, BackendError>;

    async fn subscribe(
        &self,
        params: SubscribeChangesParams,
    ) -> Result<ChangeSubscription, BackendError>;
}
