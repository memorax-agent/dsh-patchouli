import type { JsonValue } from './json.js'
import type {
  RpcMethod,
  RpcParams,
  RpcResult,
  VersionToken,
} from './rpc.js'

export interface EntityRef<TType extends string = string> {
  readonly type: TType
  readonly id: string
}

export type EntityVersion<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> =
  | {
      readonly ref: EntityRef<TType>
      readonly version: VersionToken
      readonly state: 'active'
      readonly value: TValue
    }
  | {
      readonly ref: EntityRef<TType>
      readonly version: VersionToken
      readonly state: 'deleted'
    }

export interface CreateEntityData<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> {
  readonly type: TType
  readonly id?: string
  readonly value: TValue
}

export type CreateEntityParams<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> = RpcParams<CreateEntityData<TType, TValue>>

export interface ReadEntityData<TType extends string = string> {
  readonly ref: EntityRef<TType>
}

export type ReadEntityParams<TType extends string = string> = RpcParams<
  ReadEntityData<TType>
>

export interface UpdateEntityData<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> {
  readonly ref: EntityRef<TType>
  readonly value: TValue
}

export type UpdateEntityParams<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> = RpcParams<UpdateEntityData<TType, TValue>>

export interface DeleteEntityData<TType extends string = string> {
  readonly ref: EntityRef<TType>
}

export type DeleteEntityParams<TType extends string = string> = RpcParams<
  DeleteEntityData<TType>
>

export interface MutationData<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> {
  readonly entity: EntityVersion<TType, TValue>
}

export type MutationResult<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> = RpcResult<MutationData<TType, TValue>>

export interface ReadEntityResultData<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> {
  readonly state: 'active' | 'conflicted' | 'deleted'
  readonly variants: readonly EntityVersion<TType, TValue>[]
}

export type ReadEntityResult<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> = RpcResult<ReadEntityResultData<TType, TValue>>

export interface EntityCrudContract<
  TType extends string = string,
  TValue extends JsonValue = JsonValue,
> {
  readonly 'patchouli.entity.create@1': RpcMethod<
    CreateEntityParams<TType, TValue>,
    MutationResult<TType, TValue>
  >
  readonly 'patchouli.entity.read@1': RpcMethod<
    ReadEntityParams<TType>,
    ReadEntityResult<TType, TValue>
  >
  readonly 'patchouli.entity.update@1': RpcMethod<
    UpdateEntityParams<TType, TValue>,
    MutationResult<TType, TValue>
  >
  readonly 'patchouli.entity.delete@1': RpcMethod<
    DeleteEntityParams<TType>,
    MutationResult<TType, TValue>
  >
}
