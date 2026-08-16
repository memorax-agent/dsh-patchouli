import type { EntityVersion, MutationResult } from './entity.js'
import type { ArtifactValue, FactMetadata } from './facts.js'
import type { JsonObject } from './json.js'
import type { RpcMethod, RpcParams, RpcResult, VersionToken } from './rpc.js'

export interface ArtifactUploadBeginData {
  readonly id?: string
  readonly media_type: string
  readonly name: string | null
  readonly expected_byte_length: number | null
  readonly expected_digest: string | null
  readonly metadata: FactMetadata<'patchouli.artifact@1'>
}

export type ArtifactUploadBeginParams = RpcParams<ArtifactUploadBeginData>

export interface ArtifactUploadBeginResultData {
  readonly upload_id: string
  readonly max_chunk_bytes: number
}

export type ArtifactUploadBeginResult = RpcResult<ArtifactUploadBeginResultData>

export interface ArtifactUploadChunkData {
  readonly upload_id: string
  readonly offset: number
  readonly bytes_base64: string
}

export type ArtifactUploadChunkParams = RpcParams<ArtifactUploadChunkData>

export interface ArtifactUploadChunkResultData {
  readonly next_offset: number
}

export type ArtifactUploadChunkResult = RpcResult<ArtifactUploadChunkResultData>

export interface ArtifactUploadCommitData {
  readonly upload_id: string
}

export type ArtifactUploadCommitParams = RpcParams<ArtifactUploadCommitData>
export type ArtifactUploadCommitResult = MutationResult<'artifact', ArtifactValue & JsonObject>

export interface ArtifactDownloadChunkData {
  readonly id: string
  readonly version: VersionToken | null
  readonly offset: number
  readonly max_bytes: number
}

export type ArtifactDownloadChunkParams = RpcParams<ArtifactDownloadChunkData>

export interface ArtifactDownloadChunkResultData {
  readonly entity: EntityVersion<'artifact', ArtifactValue & JsonObject>
  readonly offset: number
  readonly next_offset: number
  readonly eof: boolean
  readonly bytes_base64: string
}

export type ArtifactDownloadChunkResult = RpcResult<ArtifactDownloadChunkResultData>

export interface ArtifactTransferContract {
  readonly 'patchouli.artifact.upload.begin@1': RpcMethod<
    ArtifactUploadBeginParams,
    ArtifactUploadBeginResult
  >
  readonly 'patchouli.artifact.upload.chunk@1': RpcMethod<
    ArtifactUploadChunkParams,
    ArtifactUploadChunkResult
  >
  readonly 'patchouli.artifact.upload.commit@1': RpcMethod<
    ArtifactUploadCommitParams,
    ArtifactUploadCommitResult
  >
  readonly 'patchouli.artifact.download.chunk@1': RpcMethod<
    ArtifactDownloadChunkParams,
    ArtifactDownloadChunkResult
  >
}
