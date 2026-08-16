import type { VersionToken } from './rpc.js'
import type { EntityRef } from './entity.js'

export const errorCodes = {
  invalidRequest: -32602,
  unauthenticated: -32001,
  forbidden: -32002,
  notFound: -32003,
  versionConflict: -32004,
  idempotencyConflict: -32005,
  unsupportedCapability: -32006,
  deadlineExceeded: -32007,
  overloaded: -32009,
  cursorExpired: -32010,
  workUnitExpired: -32011,
} as const

export type ErrorReason =
  | 'CURSOR_EXPIRED'
  | 'DEADLINE_EXCEEDED'
  | 'FORBIDDEN'
  | 'IDEMPOTENCY_CONFLICT'
  | 'INVALID_REQUEST'
  | 'NOT_FOUND'
  | 'OVERLOADED'
  | 'UNAUTHENTICATED'
  | 'UNSUPPORTED_CAPABILITY'
  | 'VERSION_CONFLICT'
  | 'WORK_UNIT_EXPIRED'

export interface ProtocolErrorData {
  readonly reason: ErrorReason
  readonly current_versions?: readonly VersionToken[]
  readonly conflicts?: readonly EntityVersionConflict[]
}

export interface EntityVersionConflict {
  readonly ref: EntityRef
  readonly current_versions: readonly VersionToken[]
}
