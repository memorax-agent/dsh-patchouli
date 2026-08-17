import type {
  MemoryData,
  MemoryPlugin,
  MemoryPluginContext,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  recordOf,
  requireQuery,
  stringValue,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

type MemoryEvolveTrack = 'memory' | 'user' | 'daily' | 'project' | 'key'

interface MemoryEvolveAgent {
  readonly id?: string
  readonly session: {
    readonly id?: string
    readonly header: Readonly<Record<string, MemoryData>> & { readonly cwd: string }
  }
}

export interface MemoryEvolveNative {
  snapshot(agent: MemoryEvolveAgent): MemoryData
  query(
    target: MemoryEvolveTrack,
    agent: MemoryEvolveAgent,
    options: Record<string, unknown>,
  ): MemoryData
}

const tracks = new Set<MemoryEvolveTrack>(['memory', 'user', 'daily', 'project', 'key'])

const snapshotTracks = ['memory', 'user', 'key'] as const

/** Expose native reads while keeping all writes behind memory-evolve's approved tool. */
export function createMemoryEvolveAdapter(native: MemoryEvolveNative): MemoryPlugin {
  return {
    id: 'memory-evolve',
    filter: goojfcRouteFilters['memory-evolve'],
    async update() {
      throw new Error('memory-evolve writes require its native approval-aware memory tool')
    },
    async retrieve(request, context) {
      return queryStore(native, request, context)
    },
  }
}

async function queryStore(
  native: MemoryEvolveNative,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const data = recordOf(request.data)
  if (request.meta.attributes?.point === 'agent/pre-step') {
    const result = native.snapshot(agentOf(request))
    context.signal?.throwIfAborted()
    return result
  }
  const agent = agentOf(request)
  const params = data ?? {}
  const target = stringValue(params.target)
  if (target === undefined) {
    const filter = requireQuery(request.data, 'memory-evolve')
    const limit = positiveLimit(params.limit, 20)
    const results = Object.fromEntries(snapshotTracks.map(track => [
      track,
      native.query(track, agent, { filter, limit }),
    ]))
    context.signal?.throwIfAborted()
    return { tracks: results }
  }
  const result = native.query(requiredTrack(target), agent, {
    ...(stringValue(params.query) === undefined ? {} : { filter: stringValue(params.query) }),
    ...(typeof params.since === 'string' ? { since: params.since } : {}),
    ...(typeof params.until === 'string' ? { until: params.until } : {}),
    ...(params.limit === undefined ? {} : { limit: positiveLimit(params.limit) }),
    ...(typeof params.recent === 'boolean' ? { recent: params.recent } : {}),
    ...(stringValue(params.branch) === undefined ? {} : { branch: stringValue(params.branch) }),
  })
  context.signal?.throwIfAborted()
  return result
}

function positiveLimit(value: unknown, fallback?: number): number {
  if (Number.isSafeInteger(value) && typeof value === 'number' && value > 0) return value
  if (value === undefined && fallback !== undefined) return fallback
  throw new TypeError('memory-evolve limit must be a positive safe integer')
}

function agentOf(request: MemoryUpdateRequest | MemoryRetrieveRequest): MemoryEvolveAgent {
  const workspaceRoot = request.meta.attributes?.workspaceRoot
  if (typeof workspaceRoot !== 'string' || workspaceRoot.trim() === '') {
    throw new TypeError('memory-evolve requires meta.attributes.workspaceRoot')
  }
  const observation = recordOf(request.data)
  const observedAgent = recordOf(observation?.agent)
  const observedSession = recordOf(observation?.session)
  const observedHeader = recordOf(observedSession?.header)
  const sessionId = stringValue(observedHeader?.id)
    ?? stringValue(request.meta.attributes?.sessionId)
  const agentId = stringValue(observedAgent?.id) ?? sessionId
  return {
    ...(agentId === undefined ? {} : { id: agentId }),
    session: {
      ...(sessionId === undefined ? {} : { id: sessionId }),
      header: { ...observedHeader, cwd: workspaceRoot },
    },
  }
}

function requiredTrack(value: unknown): MemoryEvolveTrack {
  const target = stringValue(value)
  if (target === undefined || !tracks.has(target as MemoryEvolveTrack)) {
    throw new TypeError('memory-evolve requires a valid explicit target')
  }
  return target as MemoryEvolveTrack
}
