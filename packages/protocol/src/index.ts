export * from './artifact.js'
export * from './changes.js'
export * from './entity.js'
export * from './errors.js'
export * from './facts.js'
export * from './json.js'
export * from './rpc.js'

import type { ReactiveContract } from './changes.js'
import type { ArtifactTransferContract } from './artifact.js'
import type { EntityCrudContract } from './entity.js'
import type { JsonValue } from './json.js'
import type {
  ControlShutdownParams,
  ControlShutdownResult,
  ControlCheckpointParams,
  ControlCheckpointResult,
  ControlStatusParams,
  ControlStatusResult,
  HandshakeParams,
  HandshakeResult,
  RpcMethod,
} from './rpc.js'

export type PatchouliProtocol<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> = EntityCrudContract<TType, TValue>
  & ArtifactTransferContract
  & ReactiveContract<TType>
  & {
    readonly 'patchouli.protocol.handshake@1': RpcMethod<HandshakeParams, HandshakeResult>
    readonly 'patchouli.control.status@1': RpcMethod<ControlStatusParams, ControlStatusResult>
    readonly 'patchouli.control.checkpoint@1': RpcMethod<ControlCheckpointParams, ControlCheckpointResult>
    readonly 'patchouli.control.shutdown@1': RpcMethod<ControlShutdownParams, ControlShutdownResult>
  }
