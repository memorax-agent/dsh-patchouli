import type { EntityRef } from './entity.js'
import type {
  ChangeCursor,
  RpcMethod,
  RpcParams,
  RpcResult,
  VersionToken,
} from './rpc.js'

export type ChangeKind = 'conflicted' | 'created' | 'deleted' | 'resolved' | 'updated'

export interface ChangeFilter<TType extends string = string> {
  readonly types?: readonly TType[]
  readonly ids?: readonly string[]
}

export interface SubscribeChangesData<TType extends string = string> {
  readonly filter?: ChangeFilter<TType>
  readonly after_cursor?: ChangeCursor
}

export type SubscribeChangesParams<TType extends string = string> = RpcParams<
  SubscribeChangesData<TType>
>

export interface SubscribeChangesResultData {
  readonly subscription_id: string
  readonly cursor: ChangeCursor
}

export type SubscribeChangesResult = RpcResult<SubscribeChangesResultData>

export interface UnsubscribeChangesData {
  readonly subscription_id: string
}

export type UnsubscribeChangesParams = RpcParams<UnsubscribeChangesData>

export interface UnsubscribeChangesResultData {
  readonly removed: boolean
}

export type UnsubscribeChangesResult = RpcResult<UnsubscribeChangesResultData>

export interface ChangeRecord<TType extends string = string> {
  readonly cursor: ChangeCursor
  readonly ref: EntityRef<TType>
  readonly kind: ChangeKind
  readonly head_versions: readonly VersionToken[]
}

export interface ChangesEventData<TType extends string = string> {
  readonly subscription_id: string
  readonly change: ChangeRecord<TType>
}

export type ChangesEventParams<TType extends string = string> = RpcParams<
  ChangesEventData<TType>
>

export interface ReactiveContract<TType extends string = string> {
  readonly 'patchouli.changes.subscribe@1': RpcMethod<
    SubscribeChangesParams<TType>,
    SubscribeChangesResult
  >
  readonly 'patchouli.changes.unsubscribe@1': RpcMethod<
    UnsubscribeChangesParams,
    UnsubscribeChangesResult
  >
  readonly 'patchouli.changes.event@1': {
    readonly params: ChangesEventParams<TType>
  }
}
