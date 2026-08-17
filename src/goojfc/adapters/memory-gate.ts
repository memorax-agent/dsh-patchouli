import type {
  MemoryData,
  MemoryPlugin,
  MemoryPluginContext,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  contentText,
  eventsOf,
  queryOf,
  recordOf,
  requireContent,
  requireQuery,
  stringArray,
  stringValue,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

type GateScope = 'session' | 'workspace' | 'global'
type GateKind = 'preference' | 'constraint' | 'fact' | 'procedure' | 'warning'

interface GateRecall {
  readonly runId: string
  readonly text: string
  readonly claimIds: readonly string[]
}

export interface MemoryGateService {
  readonly config: { readonly automaticExtraction: boolean }
  remember(content: string, options: {
    scope: GateScope
    scopeKey: string
    kind?: GateKind
    tags?: string[]
    origin?: 'explicit' | 'heuristic'
    sourceSessionId?: string
    sourceEventSeq?: number
  }): MemoryData
  prepareRecall(context: {
    query: string
    sessionId: string
    sessionScopeKey: string
    workspaceKey?: string
  }): GateRecall | undefined
  extractAndRemember(text: string, context: {
    sessionId: string
    sessionScopeKey: string
    workspaceKey?: string
    sourceEventSeq?: number
  }): readonly MemoryData[]
}

export interface MemoryGateAdapterOptions {
  sessionScopeKey(sessionId: string): string
  workspaceScopeKey(path: string): string | undefined
  recordInjection(recall: GateRecall, context: {
    readonly sessionId: string
    readonly injectionId: string
  }): void
}

const scopes = new Set<GateScope>(['session', 'workspace', 'global'])
const kinds = new Set<GateKind>(['preference', 'constraint', 'fact', 'procedure', 'warning'])

/** Add explicit Patchouli calls through memory-gate's native authority model. */
export function createMemoryGateAdapter(
  service: MemoryGateService,
  options: MemoryGateAdapterOptions,
): MemoryPlugin {
  return {
    id: 'memory-gate',
    filter: goojfcRouteFilters['memory-gate'],
    async update(request, context) {
      return updateMemoryGate(service, options, request, context)
    },
    async retrieve(request, context) {
      return retrieveMemoryGate(service, options, request, context)
    },
  }
}

async function updateMemoryGate(
  service: MemoryGateService,
  options: MemoryGateAdapterOptions,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const data = recordOf(request.data)
  const sessionId = stringAttribute(request, 'sessionId')
    ?? `${request.meta.source.type}:${request.meta.source.id}`
  const workspaceKey = workspaceKeyOf(options, request)
  if (request.meta.attributes?.point === 'session/turn-end') {
    if (!service.config.automaticExtraction) return null
    const claims = eventsOf(request.data).flatMap((value) => {
      const event = recordOf(value)
      const eventData = recordOf(event?.data)
      if (event?.type !== 'user/message'
        || recordOf(eventData?.source)?.kind !== 'user') return []
      const text = contentText(eventData?.content)
      if (text === '') return []
      return service.extractAndRemember(text, {
        sessionId,
        sessionScopeKey: options.sessionScopeKey(sessionId),
        workspaceKey,
        ...(Number.isInteger(event.seq) ? { sourceEventSeq: Number(event.seq) } : {}),
      })
    })
    return { claims }
  }
  const content = requireContent(request.data, 'memory-gate')
  const requestedScope = data?.scope
  const scope = typeof requestedScope === 'string' && scopes.has(requestedScope as GateScope)
    ? requestedScope as GateScope
    : workspaceKey === undefined ? 'session' : 'workspace'
  if (scope === 'workspace' && workspaceKey === undefined) {
    throw new TypeError('memory-gate workspace scope requires meta.attributes.workspaceRoot')
  }
  const requestedKind = data?.kind
  const kind = typeof requestedKind === 'string' && kinds.has(requestedKind as GateKind)
    ? requestedKind as GateKind
    : 'fact'
  const result = service.remember(content, {
    scope,
    scopeKey: scope === 'global'
      ? 'global'
      : scope === 'workspace' ? workspaceKey! : options.sessionScopeKey(sessionId),
    kind,
    tags: stringArray(data?.tags),
    origin: 'explicit',
    sourceSessionId: sessionId,
    ...(Number.isInteger(data?.sourceEventSeq)
      ? { sourceEventSeq: Number(data?.sourceEventSeq) }
      : {}),
  })
  context.signal?.throwIfAborted()
  return result
}

async function retrieveMemoryGate(
  service: MemoryGateService,
  options: MemoryGateAdapterOptions,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  if (request.meta.attributes?.point === 'agent/pre-step'
    && request.meta.attributes.step !== 1) return null
  const sessionId = stringAttribute(request, 'sessionId')
    ?? `${request.meta.source.type}:${request.meta.source.id}`
  const query = request.meta.attributes?.point === 'agent/pre-step'
    ? queryOf(request.data)
    : requireQuery(request.data, 'memory-gate')
  if (query === undefined) return null
  const result = service.prepareRecall({
    query,
    sessionId,
    sessionScopeKey: options.sessionScopeKey(sessionId),
    workspaceKey: workspaceKeyOf(options, request),
  })
  context.signal?.throwIfAborted()
  if (result === undefined) return null
  const turn = request.meta.attributes?.turn ?? 'unknown'
  const step = request.meta.attributes?.step ?? 'unknown'
  options.recordInjection(result, {
    sessionId,
    injectionId: request.meta.requestId
      ?? `patchouli:${sessionId}:${String(turn)}:${String(step)}:${result.runId}`,
  })
  return result.text
}

function workspaceKeyOf(
  options: MemoryGateAdapterOptions,
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
): string | undefined {
  const root = stringAttribute(request, 'workspaceRoot')
  return root === undefined ? undefined : options.workspaceScopeKey(root)
}

function stringAttribute(
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
  key: string,
): string | undefined {
  return stringValue(request.meta.attributes?.[key])
}
