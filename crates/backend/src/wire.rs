use serde::{Deserialize, Serialize};

use crate::{ChangeRecord, EntityRef, RpcParams, RpcResult, VersionToken};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(i64),
    String(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: JsonRpcVersion,
    pub id: JsonRpcId,
    pub method: String,
    pub params: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcNotification<T> {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    pub params: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcSuccess<T> {
    pub jsonrpc: JsonRpcVersion,
    pub id: JsonRpcId,
    pub result: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcFailure<T = ProtocolErrorData> {
    pub jsonrpc: JsonRpcVersion,
    pub id: Option<JsonRpcId>,
    pub error: JsonRpcError<T>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum JsonRpcVersion {
    #[default]
    #[serde(rename = "2.0")]
    V2,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonRpcError<T = ProtocolErrorData> {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeParams {
    pub client: ClientIdentity,
    pub protocol_versions: Vec<u16>,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientIdentity {
    pub name: String,
    pub version: String,
    pub instance_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeResult {
    pub protocol_version: u16,
    pub server: ServerIdentity,
    pub capabilities: Vec<String>,
    pub limits: ServerLimits,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerIdentity {
    pub version: String,
    pub cluster_id: String,
    pub node_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerLimits {
    pub max_request_bytes: u64,
    pub max_result_items: u64,
    pub idempotency_retention_seconds: u64,
    pub change_retention_seconds: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyData {}

pub type ControlStatusParams = RpcParams<EmptyData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlStatusResultData {
    pub ready: bool,
    pub provider: String,
    pub generation: u64,
    pub recovered_after_unclean_shutdown: bool,
    pub pid: u32,
    pub started_at_unix_ms: u64,
    pub active_connections: u64,
}

pub type ControlStatusResult = RpcResult<ControlStatusResultData>;

pub type ControlCheckpointParams = RpcParams<EmptyData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlCheckpointResultData {
    pub completed: bool,
}

pub type ControlCheckpointResult = RpcResult<ControlCheckpointResultData>;

pub type ControlShutdownParams = RpcParams<EmptyData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlShutdownResultData {
    pub accepted: bool,
}

pub type ControlShutdownResult = RpcResult<ControlShutdownResultData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeChangesResultData {
    pub subscription_id: String,
    pub cursor: String,
}

pub type SubscribeChangesResult = RpcResult<SubscribeChangesResultData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsubscribeChangesData {
    pub subscription_id: String,
}

pub type UnsubscribeChangesParams = RpcParams<UnsubscribeChangesData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsubscribeChangesResultData {
    pub removed: bool,
}

pub type UnsubscribeChangesResult = RpcResult<UnsubscribeChangesResultData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesEventData {
    pub subscription_id: String,
    pub change: ChangeRecord,
}

pub type ChangesEventParams = RpcParams<ChangesEventData>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtocolErrorReason {
    Cancelled,
    CursorExpired,
    DeadlineExceeded,
    Forbidden,
    IdempotencyConflict,
    InvalidRequest,
    NotFound,
    Overloaded,
    Unauthenticated,
    UnsupportedCapability,
    VersionConflict,
    WorkUnitExpired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolErrorData {
    pub reason: ProtocolErrorReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_versions: Option<Vec<VersionToken>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflicts: Option<Vec<ProtocolEntityConflict>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolEntityConflict {
    #[serde(rename = "ref")]
    pub entity_ref: EntityRef,
    pub current_versions: Vec<VersionToken>,
}

pub mod error_codes {
    pub const INVALID_REQUEST: i32 = -32602;
    pub const UNAUTHENTICATED: i32 = -32001;
    pub const FORBIDDEN: i32 = -32002;
    pub const NOT_FOUND: i32 = -32003;
    pub const VERSION_CONFLICT: i32 = -32004;
    pub const IDEMPOTENCY_CONFLICT: i32 = -32005;
    pub const UNSUPPORTED_CAPABILITY: i32 = -32006;
    pub const DEADLINE_EXCEEDED: i32 = -32007;
    pub const CANCELLED: i32 = -32008;
    pub const OVERLOADED: i32 = -32009;
    pub const CURSOR_EXPIRED: i32 = -32010;
    pub const WORK_UNIT_EXPIRED: i32 = -32011;
}
