import type { JsonObject, JsonValue } from './json.js'

export const protocolVersion = 1 as const

export const methods = {
  handshake: 'patchouli.protocol.handshake@1',
  entityCreate: 'patchouli.entity.create@1',
  entityRead: 'patchouli.entity.read@1',
  entityUpdate: 'patchouli.entity.update@1',
  entityDelete: 'patchouli.entity.delete@1',
  changesSubscribe: 'patchouli.changes.subscribe@1',
  changesUnsubscribe: 'patchouli.changes.unsubscribe@1',
  changesEvent: 'patchouli.changes.event@1',
} as const

export type Method = typeof methods[keyof typeof methods]
export type JsonRpcId = number | string

export interface JsonRpcRequest<TParams = JsonObject> {
  readonly jsonrpc: '2.0'
  readonly id: JsonRpcId
  readonly method: string
  readonly params: TParams
}

export interface JsonRpcNotification<TParams = JsonObject> {
  readonly jsonrpc: '2.0'
  readonly method: string
  readonly params: TParams
}

export interface JsonRpcSuccess<TResult = JsonValue> {
  readonly jsonrpc: '2.0'
  readonly id: JsonRpcId
  readonly result: TResult
}

export interface JsonRpcFailure<TData = JsonValue> {
  readonly jsonrpc: '2.0'
  readonly id: JsonRpcId | null
  readonly error: {
    readonly code: number
    readonly message: string
    readonly data?: TData
  }
}

export type ChangeCursor = string
export type VersionToken = string

export type Meta = JsonObject

export interface RpcParams<TData> {
  readonly meta: Meta
  readonly data: TData
}

export interface RpcResult<TData> {
  readonly meta: Meta
  readonly data: TData
}

export interface HandshakeParams {
  readonly client: {
    readonly name: string
    readonly version: string
    readonly instance_id: string
  }
  readonly protocol_versions: readonly number[]
  readonly capabilities: readonly string[]
}

export interface HandshakeResult {
  readonly protocol_version: typeof protocolVersion
  readonly server: {
    readonly version: string
    readonly cluster_id: string
    readonly node_id: string
  }
  readonly capabilities: readonly string[]
  readonly limits: {
    readonly max_request_bytes: number
    readonly max_result_items: number
    readonly idempotency_retention_seconds: number
    readonly change_retention_seconds: number
  }
}

export interface RpcMethod<TParams, TResult> {
  readonly params: TParams
  readonly result: TResult
}
