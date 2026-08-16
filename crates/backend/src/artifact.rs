use serde::{Deserialize, Serialize};

use crate::{ArtifactSchemaVersion, EntityVersion, FactMetadata, RpcParams, RpcResult};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactUploadBeginData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub media_type: String,
    pub name: Option<String>,
    pub expected_byte_length: Option<u64>,
    pub expected_digest: Option<String>,
    pub metadata: FactMetadata<ArtifactSchemaVersion>,
}

pub type ArtifactUploadBeginParams = RpcParams<ArtifactUploadBeginData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadBeginResultData {
    pub upload_id: String,
    pub max_chunk_bytes: u64,
}

pub type ArtifactUploadBeginResult = RpcResult<ArtifactUploadBeginResultData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadChunkData {
    pub upload_id: String,
    pub offset: u64,
    pub bytes_base64: String,
}

pub type ArtifactUploadChunkParams = RpcParams<ArtifactUploadChunkData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadChunkResultData {
    pub next_offset: u64,
}

pub type ArtifactUploadChunkResult = RpcResult<ArtifactUploadChunkResultData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadCommitData {
    pub upload_id: String,
}

pub type ArtifactUploadCommitParams = RpcParams<ArtifactUploadCommitData>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDownloadChunkData {
    pub id: String,
    pub version: Option<String>,
    pub offset: u64,
    pub max_bytes: u64,
}

pub type ArtifactDownloadChunkParams = RpcParams<ArtifactDownloadChunkData>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactDownloadChunkResultData {
    pub entity: EntityVersion,
    pub offset: u64,
    pub next_offset: u64,
    pub eof: bool,
    pub bytes_base64: String,
}

pub type ArtifactDownloadChunkResult = RpcResult<ArtifactDownloadChunkResultData>;
