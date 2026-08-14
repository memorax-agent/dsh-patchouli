import type { VersionToken } from './rpc.js'

export const errorCodes = {
  unauthenticated: -32001,
  forbidden: -32002,
  notFound: -32003,
  versionConflict: -32004,
  idempotencyConflict: -32005,
  unsupportedCapability: -32006,
  deadlineExceeded: -32007,
  cancelled: -32008,
  overloaded: -32009,
  cursorExpired: -32010,
} as const

export type ErrorReason =
  | 'CANCELLED'
  | 'CURSOR_EXPIRED'
  | 'DEADLINE_EXCEEDED'
  | 'FORBIDDEN'
  | 'IDEMPOTENCY_CONFLICT'
  | 'NOT_FOUND'
  | 'OVERLOADED'
  | 'UNAUTHENTICATED'
  | 'UNSUPPORTED_CAPABILITY'
  | 'VERSION_CONFLICT'

export interface ProtocolErrorData {
  readonly reason: ErrorReason
  readonly current_versions?: readonly VersionToken[]
}
