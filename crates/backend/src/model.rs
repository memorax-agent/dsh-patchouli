use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{BackendError, BackendErrorReason};

pub type Meta = BTreeMap<String, Value>;
pub type ChangeCursor = String;
pub type VersionToken = String;
pub const DEADLINE_META_FIELD: &str = "deadline_unix_ms";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RequestDeadline(Option<u64>);

impl RequestDeadline {
    pub fn from_meta(meta: &Meta) -> Result<Self, BackendError> {
        let Some(value) = meta.get(DEADLINE_META_FIELD) else {
            return Ok(Self(None));
        };
        let deadline = value.as_u64().ok_or_else(|| {
            BackendError::new(
                BackendErrorReason::InvalidRequest,
                format!(
                    "meta.{DEADLINE_META_FIELD} must be an unsigned Unix timestamp in milliseconds"
                ),
            )
        })?;
        Ok(Self(Some(deadline)))
    }

    pub fn check(self, now_unix_ms: u64) -> Result<(), BackendError> {
        if self.0.is_some_and(|deadline| now_unix_ms >= deadline) {
            return Err(BackendError::new(
                BackendErrorReason::DeadlineExceeded,
                "request deadline elapsed before acceptance",
            ));
        }
        Ok(())
    }

    pub fn check_now(self) -> Result<(), BackendError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                BackendError::new(BackendErrorReason::InvalidRequest, error.to_string())
            })?
            .as_millis() as u64;
        self.check(now)
    }

    pub fn unix_ms(self) -> Option<u64> {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcParams<T> {
    pub meta: Meta,
    pub data: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcResult<T> {
    pub meta: Meta,
    pub data: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    #[serde(rename = "type")]
    pub entity_type: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EntityVersion {
    Active {
        #[serde(rename = "ref")]
        entity_ref: EntityRef,
        version: VersionToken,
        value: Value,
    },
    Deleted {
        #[serde(rename = "ref")]
        entity_ref: EntityRef,
        version: VersionToken,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateEntityData {
    #[serde(rename = "type")]
    pub entity_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub value: Value,
}

pub type CreateEntityParams = RpcParams<CreateEntityData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadEntityData {
    #[serde(rename = "ref")]
    pub entity_ref: EntityRef,
}

pub type ReadEntityParams = RpcParams<ReadEntityData>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateEntityData {
    #[serde(rename = "ref")]
    pub entity_ref: EntityRef,
    pub value: Value,
}

pub type UpdateEntityParams = RpcParams<UpdateEntityData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteEntityData {
    #[serde(rename = "ref")]
    pub entity_ref: EntityRef,
}

pub type DeleteEntityParams = RpcParams<DeleteEntityData>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MutationData {
    pub entity: EntityVersion,
}

pub type MutationResult = RpcResult<MutationData>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadEntityResultData {
    pub state: ReadState,
    pub variants: Vec<EntityVersion>,
}

pub type ReadEntityResult = RpcResult<ReadEntityResultData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetrieveEntitiesData {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    #[serde(default = "default_retrieve_limit")]
    pub limit: usize,
}

fn default_retrieve_limit() -> usize {
    10
}

pub type RetrieveEntitiesParams = RpcParams<RetrieveEntitiesData>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrievalHit {
    pub score: f64,
    pub variants: Vec<EntityVersion>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetrieveEntitiesResultData {
    pub hits: Vec<RetrievalHit>,
}

pub type RetrieveEntitiesResult = RpcResult<RetrieveEntitiesResultData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadState {
    Active,
    Conflicted,
    Deleted,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeChangesData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<ChangeFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_cursor: Option<ChangeCursor>,
}

pub type SubscribeChangesParams = RpcParams<SubscribeChangesData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Conflicted,
    Created,
    Deleted,
    Resolved,
    Updated,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub cursor: ChangeCursor,
    #[serde(rename = "ref")]
    pub entity_ref: EntityRef,
    pub kind: ChangeKind,
    pub head_versions: Vec<VersionToken>,
}
