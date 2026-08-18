use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ProviderError {
    reason: ProviderErrorReason,
    message: String,
}

impl ProviderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            reason: ProviderErrorReason::Internal,
            message: message.into(),
        }
    }

    pub fn deadline_exceeded() -> Self {
        Self::with_reason(
            ProviderErrorReason::DeadlineExceeded,
            "request deadline elapsed before acceptance",
        )
    }

    pub fn with_reason(reason: ProviderErrorReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }

    pub fn context(self, context: impl std::fmt::Display) -> Self {
        Self::with_reason(self.reason, format!("{context}: {}", self.message))
    }

    pub fn reason(&self) -> ProviderErrorReason {
        self.reason
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorReason {
    Internal,
    DeadlineExceeded,
    Unauthenticated,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRecovery {
    pub generation: u64,
    pub recovered_after_unclean_shutdown: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub authority: bool,
    pub replica: bool,
    pub change_stream: bool,
    pub retrieval: bool,
    pub idempotency: bool,
    pub work_units: bool,
    pub causal_reads: bool,
    pub monotonic_reads: bool,
    pub read_your_writes: bool,
    pub linearizable_reads: bool,
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
            causal_reads: false,
            monotonic_reads: false,
            read_your_writes: false,
            linearizable_reads: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencySource {
    Authority,
    Replica,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityKey {
    pub scope_json: String,
    pub entity_type: String,
    pub entity_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredVersionState {
    Active,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCrdtChange {
    pub hash: String,
    pub parents: Vec<String>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCrdtField {
    pub path: String,
    pub heads: Vec<String>,
    pub changes: Vec<StoredCrdtChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEntityVersion {
    pub version: String,
    pub state: StoredVersionState,
    pub value_json: Option<String>,
    pub crdt_fields: Vec<StoredCrdtField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub head_versions: Vec<String>,
    pub versions: Vec<StoredEntityVersion>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeQuery {
    pub scope_json: String,
    pub entity_types: Option<Vec<String>>,
    pub entity_ids: Option<Vec<String>>,
    pub after_cursor: u64,
    pub limit: usize,
    pub retained_after_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredChange {
    pub cursor: u64,
    pub entity_type: String,
    pub entity_id: String,
    pub kind: StoredChangeKind,
    pub head_versions: Vec<String>,
    pub event_meta_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangePage {
    pub oldest_cursor: Option<u64>,
    pub current_cursor: u64,
    pub changes: Vec<StoredChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionConsistency {
    pub key_json: String,
    pub monotonic_reads: bool,
    pub read_your_writes: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadConsistency {
    pub allowed_sources: Vec<ConsistencySource>,
    pub minimum_tokens: Vec<String>,
    pub sessions: Vec<SessionConsistency>,
    pub linearization_keys: Vec<String>,
    pub deadline_unix_ms: Option<u64>,
}

impl ReadConsistency {
    pub fn authority() -> Self {
        Self {
            allowed_sources: vec![ConsistencySource::Authority],
            minimum_tokens: Vec::new(),
            sessions: Vec::new(),
            linearization_keys: Vec::new(),
            deadline_unix_ms: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadObservation {
    pub source: ConsistencySource,
    pub frontier: u64,
    pub causal_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsistentRead<T> {
    Read {
        value: T,
        observation: ReadObservation,
    },
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrieveQuery {
    pub scope_json: String,
    pub entity_types: Option<Vec<String>>,
    pub text: Option<String>,
    pub entity_ids: Option<Vec<String>>,
    pub filters: Vec<RetrieveFilter>,
    pub order: RetrieveOrder,
    pub fingerprint: String,
    pub after: Option<RetrieveCursor>,
    pub limit: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrieveFilterOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Contains,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrieveFilter {
    pub pointer: String,
    pub operator: RetrieveFilterOperator,
    pub value_json: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrieveOrder {
    Relevance,
    Newest,
    Oldest,
    IdAscending,
    IdDescending,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrieveCursor {
    pub order: RetrieveOrder,
    pub query_fingerprint: String,
    pub score: f64,
    pub recorded_at_unix_ms: u64,
    pub entity_type: String,
    pub entity_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievedEntity {
    pub key: EntityKey,
    pub snapshot: EntitySnapshot,
    pub score: f64,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievedPage {
    pub entities: Vec<RetrievedEntity>,
    pub next_cursor: Option<RetrieveCursor>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredChangeKind {
    Conflicted,
    Created,
    Deleted,
    Resolved,
    Updated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityCommit {
    pub key: EntityKey,
    pub expected_heads: Vec<String>,
    pub new_versions: Vec<StoredEntityVersion>,
    pub head_versions: Vec<String>,
    pub change_kind: StoredChangeKind,
    pub causal_token: String,
    pub event_meta_json: String,
    pub write_session_keys: Vec<String>,
    pub ordering_key_json: Option<String>,
    pub recorded_at_unix_ms: u64,
    pub deadline_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityCommitOutcome {
    Committed,
    Conflict { current_heads: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub identity_json: String,
    pub request_json: String,
    pub result_json: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdempotencyReadOutcome {
    Missing,
    Replayed { result_json: String },
    Conflict,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IdempotentCommitOutcome {
    Committed,
    EntityConflict { current_heads: Vec<String> },
    Replayed { result_json: String },
    IdempotencyConflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkUnit {
    pub identity_json: String,
    pub scope_json: String,
    pub policy_json: String,
    pub expiry_action: WorkUnitExpiryAction,
    pub now_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub deadline_unix_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkUnitExpiryAction {
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkUnitReadOutcome {
    Open(ConsistentRead<Option<EntitySnapshot>>),
    Closing,
    PolicyMismatch,
    Committed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorkUnitRetrieveOutcome {
    Open(ConsistentRead<RetrievedPage>),
    Closing,
    PolicyMismatch,
    Committed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkUnitCommit {
    pub work_unit: WorkUnit,
    pub entity: EntityCommit,
    pub conflict_policy_json: String,
    pub idempotency: Option<IdempotencyRecord>,
    pub close: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkUnitConflict {
    pub key: EntityKey,
    pub baseline_heads: Vec<String>,
    pub staged: EntitySnapshot,
    pub current: Option<EntitySnapshot>,
    pub conflict_policy_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkUnitResolution {
    pub expected_published_heads: Vec<String>,
    pub entity: EntityCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkUnitPublish {
    pub work_unit: WorkUnit,
    pub resolutions: Vec<WorkUnitResolution>,
    pub recorded_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkUnitCommitOutcome {
    Staged,
    Published,
    Closing,
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

    async fn read_entity(
        &self,
        key: &EntityKey,
        consistency: ReadConsistency,
    ) -> Result<ConsistentRead<Option<EntitySnapshot>>, ProviderError>;

    async fn read_changes(&self, _query: ChangeQuery) -> Result<ChangePage, ProviderError> {
        Err(ProviderError::new(
            "provider does not support change queries",
        ))
    }

    async fn wait_for_changes(
        &self,
        _scope_json: &str,
        _after_cursor: u64,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::new(
            "provider does not support change notifications",
        ))
    }

    async fn retrieve_entities(
        &self,
        _query: RetrieveQuery,
        _consistency: ReadConsistency,
    ) -> Result<ConsistentRead<RetrievedPage>, ProviderError> {
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
        _scope_json: &str,
        _consistency: ReadConsistency,
        _identity_json: &str,
        _request_json: &str,
        _now_unix_ms: u64,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        Err(ProviderError::new("provider does not support idempotency"))
    }

    async fn read_idempotency_in_work_unit(
        &self,
        _work_unit: &WorkUnit,
        _consistency: ReadConsistency,
        _identity_json: &str,
        _request_json: &str,
        _now_unix_ms: u64,
        _allow_replay: bool,
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
        consistency: ReadConsistency,
    ) -> Result<WorkUnitReadOutcome, ProviderError>;

    async fn retrieve_entities_in_work_unit(
        &self,
        _work_unit: &WorkUnit,
        _query: RetrieveQuery,
        _consistency: ReadConsistency,
    ) -> Result<WorkUnitRetrieveOutcome, ProviderError> {
        Err(ProviderError::new(
            "provider does not support work-unit retrieval",
        ))
    }

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
