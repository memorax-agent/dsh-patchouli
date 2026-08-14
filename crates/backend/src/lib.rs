mod config;
mod controller;
mod error;
mod model;
mod service;
mod wire;

pub use error::{BackendError, BackendErrorReason};
pub use model::{
    ChangeCursor, ChangeFilter, ChangeKind, ChangeRecord, CreateEntityData, CreateEntityParams,
    DeleteEntityData, DeleteEntityParams, EntityRef, EntityVersion, Meta, MutationData,
    MutationResult, ReadEntityData, ReadEntityParams, ReadEntityResult, ReadEntityResultData,
    ReadState, RpcParams, RpcResult, SubscribeChangesData, SubscribeChangesParams,
    UpdateEntityData, UpdateEntityParams, VersionToken,
};
pub use service::{BackendService, ChangeStream};
pub use wire::{
    ChangesEventData, ChangesEventParams, ClientIdentity, HandshakeParams, HandshakeResult,
    JsonRpcError, JsonRpcFailure, JsonRpcId, JsonRpcNotification, JsonRpcRequest, JsonRpcSuccess,
    JsonRpcVersion, ProtocolErrorData, ProtocolErrorReason, ServerIdentity, ServerLimits,
    SubscribeChangesResult, SubscribeChangesResultData, UnsubscribeChangesData,
    UnsubscribeChangesParams, UnsubscribeChangesResult, UnsubscribeChangesResultData, error_codes,
};

pub const PROTOCOL_VERSION: u16 = 1;

pub mod methods {
    pub const HANDSHAKE: &str = "patchouli.protocol.handshake@1";
    pub const ENTITY_CREATE: &str = "patchouli.entity.create@1";
    pub const ENTITY_READ: &str = "patchouli.entity.read@1";
    pub const ENTITY_UPDATE: &str = "patchouli.entity.update@1";
    pub const ENTITY_DELETE: &str = "patchouli.entity.delete@1";
    pub const CHANGES_SUBSCRIBE: &str = "patchouli.changes.subscribe@1";
    pub const CHANGES_UNSUBSCRIBE: &str = "patchouli.changes.unsubscribe@1";
    pub const CHANGES_EVENT: &str = "patchouli.changes.event@1";
}
pub use config::{
    BackendConfig, BaselinePolicy, BaselineSource, BatchCloseCondition, BatchExpiryPolicy,
    BatchPolicy, BatchVisibility, ConfigError, ConfiguredConsistency, ConflictPolicy,
    ConflictStrategy, ConsistencyBehavior, ConsistencyPolicy, ConsistencyRule, EntityPolicy,
    FieldSelector,
};
pub use controller::{PolicyDecision, PolicyEngine, PolicyError};
