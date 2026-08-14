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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub authority: bool,
    pub replica: bool,
    pub change_stream: bool,
    pub retrieval: bool,
    pub idempotency: bool,
    pub work_units: bool,
    pub causal_sessions: bool,
}

impl ProviderCapabilities {
    pub const fn transactional_authority() -> Self {
        Self {
            authority: true,
            replica: false,
            change_stream: false,
            retrieval: false,
            idempotency: false,
            work_units: false,
            causal_sessions: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityKey {
    pub scope_json: String,
    pub entity_type: String,
    pub entity_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredVersionState {
    Active,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCrdtChange {
    pub hash: String,
    pub parents: Vec<String>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCrdtField {
    pub path: String,
    pub heads: Vec<String>,
    pub changes: Vec<StoredCrdtChange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEntityVersion {
    pub version: String,
    pub state: StoredVersionState,
    pub value_json: Option<String>,
    pub crdt_fields: Vec<StoredCrdtField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitySnapshot {
    pub head_versions: Vec<String>,
    pub versions: Vec<StoredEntityVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeQuery {
    pub scope_json: String,
    pub entity_types: Option<Vec<String>>,
    pub entity_ids: Option<Vec<String>>,
    pub after_cursor: u64,
    pub limit: usize,
    pub retained_after_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredChange {
    pub cursor: u64,
    pub entity_type: String,
    pub entity_id: String,
    pub kind: StoredChangeKind,
    pub head_versions: Vec<String>,
    pub event_meta_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangePage {
    pub oldest_cursor: Option<u64>,
    pub current_cursor: u64,
    pub changes: Vec<StoredChange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsistencyQuery {
    pub scope_json: String,
    pub minimum_tokens: Vec<String>,
    pub session_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsistencyAcquireOutcome {
    Acquired { causal_token: Option<String> },
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrieveQuery {
    pub scope_json: String,
    pub entity_types: Option<Vec<String>>,
    pub query: String,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrievedEntity {
    pub key: EntityKey,
    pub snapshot: EntitySnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredChangeKind {
    Conflicted,
    Created,
    Deleted,
    Resolved,
    Updated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityCommit {
    pub key: EntityKey,
    pub expected_heads: Vec<String>,
    pub new_versions: Vec<StoredEntityVersion>,
    pub head_versions: Vec<String>,
    pub change_kind: StoredChangeKind,
    pub causal_token: String,
    pub event_meta_json: String,
    pub session_keys: Vec<String>,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityCommitOutcome {
    Committed,
    Conflict { current_heads: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyRecord {
    pub identity_json: String,
    pub request_json: String,
    pub result_json: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdempotencyReadOutcome {
    Missing,
    Replayed { result_json: String },
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdempotentCommitOutcome {
    Committed,
    EntityConflict { current_heads: Vec<String> },
    Replayed { result_json: String },
    IdempotencyConflict,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkUnit {
    pub identity_json: String,
    pub policy_json: String,
    pub expiry_action: WorkUnitExpiryAction,
    pub now_unix_ms: u64,
    pub expires_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkUnitExpiryAction {
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkUnitReadOutcome {
    Open(Option<EntitySnapshot>),
    PolicyMismatch,
    Committed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkUnitCommit {
    pub work_unit: WorkUnit,
    pub entity: EntityCommit,
    pub conflict_policy_json: String,
    pub idempotency: Option<IdempotencyRecord>,
    pub close: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkUnitConflict {
    pub key: EntityKey,
    pub baseline_heads: Vec<String>,
    pub staged: EntitySnapshot,
    pub current: Option<EntitySnapshot>,
    pub conflict_policy_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkUnitResolution {
    pub expected_published_heads: Vec<String>,
    pub entity: EntityCommit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkUnitPublish {
    pub work_unit: WorkUnit,
    pub resolutions: Vec<WorkUnitResolution>,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkUnitCommitOutcome {
    Staged,
    Published,
    Conflict { current_heads: Vec<String> },
    PublicationConflict { conflicts: Vec<WorkUnitConflict> },
    PolicyMismatch,
    Committed,
    Expired,
    Replayed { result_json: String },
    IdempotencyConflict,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn kind(&self) -> &'static str;

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::transactional_authority()
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError>;

    async fn health_check(&self) -> Result<(), ProviderError>;

    async fn read_entity(&self, key: &EntityKey) -> Result<Option<EntitySnapshot>, ProviderError>;

    async fn acquire_consistency(
        &self,
        _query: ConsistencyQuery,
    ) -> Result<ConsistencyAcquireOutcome, ProviderError> {
        Err(ProviderError::new(
            "provider does not support causal/session consistency",
        ))
    }

    async fn read_changes(&self, _query: ChangeQuery) -> Result<ChangePage, ProviderError> {
        Err(ProviderError::new(
            "provider does not support change queries",
        ))
    }

    async fn retrieve_entities(
        &self,
        _query: RetrieveQuery,
    ) -> Result<Vec<RetrievedEntity>, ProviderError> {
        Err(ProviderError::new(
            "provider does not support entity retrieval",
        ))
    }

    async fn commit_entity(
        &self,
        commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError>;

    async fn read_idempotency(
        &self,
        _identity_json: &str,
        _request_json: &str,
        _now_unix_ms: u64,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        Err(ProviderError::new("provider does not support idempotency"))
    }

    async fn commit_entity_idempotent(
        &self,
        _commit: EntityCommit,
        _idempotency: IdempotencyRecord,
        _now_unix_ms: u64,
    ) -> Result<IdempotentCommitOutcome, ProviderError> {
        Err(ProviderError::new("provider does not support idempotency"))
    }

    async fn read_entity_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        key: &EntityKey,
    ) -> Result<WorkUnitReadOutcome, ProviderError>;

    async fn commit_entity_in_work_unit(
        &self,
        commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError>;

    async fn publish_work_unit(
        &self,
        publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError>;

    async fn checkpoint(&self) -> Result<(), ProviderError>;

    async fn shutdown(&self) -> Result<(), ProviderError>;
}
