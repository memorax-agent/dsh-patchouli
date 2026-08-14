export * from './changes.js'
export * from './entity.js'
export * from './errors.js'
export * from './json.js'
export * from './rpc.js'

import type { ReactiveContract } from './changes.js'
import type { EntityCrudContract } from './entity.js'
import type { JsonValue } from './json.js'
import type { HandshakeParams, HandshakeResult, RpcMethod } from './rpc.js'

export type PatchouliProtocol<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> = EntityCrudContract<TType, TValue>
  & ReactiveContract<TType>
  & {
    readonly 'patchouli.protocol.handshake@1': RpcMethod<HandshakeParams, HandshakeResult>
  }
