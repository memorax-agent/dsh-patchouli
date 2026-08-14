mod config;
mod conflict;
mod controller;
mod engine;
mod error;
mod fact;
mod model;
mod service;
mod wire;

pub use engine::{BackendEngine, EngineError};
pub use error::{BackendError, BackendErrorReason, EntityVersionConflict};
pub use fact::{
    AbstractionLevel, Actionability, ArtifactReference, EmbeddingArtifactMetadata,
    EmbeddingMediaType, EmbeddingMetric, EpistemicStatus, FACT_COMMON_SCHEMA_URI, FactLifecycle,
    FactMetadata, FactMetadataCore, FactOrigin, FactScope, FactTime, KNOWLEDGE_ENTITY_TYPE,
    KNOWLEDGE_RELATION_ENTITY_TYPE, KNOWLEDGE_RELATION_SCHEMA_URI, KNOWLEDGE_SCHEMA_URI,
    KnowledgeContent, KnowledgeEntityType, KnowledgeProfile, KnowledgeRef,
    KnowledgeRelationSchemaVersion, KnowledgeRelationType, KnowledgeRelationValue,
    KnowledgeSchemaVersion, KnowledgeValue, LifecycleStatus, Ownership, Persistence, Provenance,
    ProvenanceKind, RetrievalMode, TemporalGrounding,
};
pub use model::{
    ChangeCursor, ChangeFilter, ChangeKind, ChangeRecord, CreateEntityData, CreateEntityParams,
    DeleteEntityData, DeleteEntityParams, EntityRef, EntityVersion, Meta, MutationData,
    MutationResult, ReadEntityData, ReadEntityParams, ReadEntityResult, ReadEntityResultData,
    ReadState, RetrievalHit, RetrieveEntitiesData, RetrieveEntitiesParams, RetrieveEntitiesResult,
    RetrieveEntitiesResultData, RpcParams, RpcResult, SubscribeChangesData, SubscribeChangesParams,
    UpdateEntityData, UpdateEntityParams, VersionToken,
};
pub use service::{BackendService, ChangeStream, ChangeSubscription, PublishedChange};
pub use wire::{
    ChangesEventData, ChangesEventParams, ClientIdentity, ControlCheckpointParams,
    ControlCheckpointResult, ControlCheckpointResultData, ControlShutdownParams,
    ControlShutdownResult, ControlShutdownResultData, ControlStatusParams, ControlStatusResult,
    ControlStatusResultData, EmptyData, HandshakeParams, HandshakeResult, JsonRpcError,
    JsonRpcFailure, JsonRpcId, JsonRpcNotification, JsonRpcRequest, JsonRpcSuccess, JsonRpcVersion,
    ProtocolEntityConflict, ProtocolErrorData, ProtocolErrorReason, ServerIdentity, ServerLimits,
    SubscribeChangesResult, SubscribeChangesResultData, UnsubscribeChangesData,
    UnsubscribeChangesParams, UnsubscribeChangesResult, UnsubscribeChangesResultData, error_codes,
};

pub const PROTOCOL_VERSION: u16 = 1;

pub mod methods {
    pub const HANDSHAKE: &str = "patchouli.protocol.handshake@1";
    pub const CONTROL_STATUS: &str = "patchouli.control.status@1";
    pub const CONTROL_CHECKPOINT: &str = "patchouli.control.checkpoint@1";
    pub const CONTROL_SHUTDOWN: &str = "patchouli.control.shutdown@1";
    pub const ENTITY_CREATE: &str = "patchouli.entity.create@1";
    pub const ENTITY_READ: &str = "patchouli.entity.read@1";
    pub const ENTITY_RETRIEVE: &str = "patchouli.entity.retrieve@1";
    pub const ENTITY_UPDATE: &str = "patchouli.entity.update@1";
    pub const ENTITY_DELETE: &str = "patchouli.entity.delete@1";
    pub const CHANGES_SUBSCRIBE: &str = "patchouli.changes.subscribe@1";
    pub const CHANGES_UNSUBSCRIBE: &str = "patchouli.changes.unsubscribe@1";
    pub const CHANGES_EVENT: &str = "patchouli.changes.event@1";
}
pub use config::{
    AcquirePolicy, AcquireRequirement, BackendConfig, BatchCloseCondition, BatchExpiryPolicy,
    Behavior, CommitConsistencyPolicy, CommitOrderingPolicy, ConfigError, ConflictFallback,
    ConflictMergeRule, ConflictMergeStrategy, ConflictPolicy, ConflictStrategy, ConsistencyPolicy,
    ConsistencySource, EntityIdentityPolicy, EntityPolicy, IdempotencyPolicy, MetaField,
    PolicyRule, PublicationPolicy, RuleMatch, SessionGuarantee, SessionPolicy, SnapshotPolicy,
};
pub use conflict::{
    ConflictCandidate, ConflictError, ConflictResolution, CrdtChange, CrdtDocument,
    resolve_conflict, resolve_prepared_conflict,
};
pub use controller::{
    CausalConsistencyPlan, ConflictPlan, ConsistencyPlan, ControlKey, PolicyError, PolicySelection,
    PolicySelector, SessionConsistencyPlan,
};
