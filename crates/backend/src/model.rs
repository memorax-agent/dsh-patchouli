use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type Meta = BTreeMap<String, Value>;
pub type ChangeCursor = String;
pub type VersionToken = String;

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
