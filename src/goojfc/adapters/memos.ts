import type { JsonValue } from '@memorax-agent/patchouli-protocol'

import type {
  MemoryPlugin,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  eventsOf,
  messagesOf,
  queryOf,
  recordOf,
  requireContent,
  stringValue,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

interface MemosNamespace {
  agentKind: string
  profileId: string
  profileLabel: string
  workspacePath?: string
  sessionKey: string
}

interface MemosRoute {
  provider: string
  model: string
  reasoningEffort?: string
  sessionId: string
}

interface MemosCore {
  prepareTurn(input: unknown): Promise<{ sessionId: string; episodeId: string }>
  onTurnEnd(input: unknown): Promise<JsonValue>
  searchMemory(
    input: unknown,
    execution?: { signal?: AbortSignal; foreground?: boolean },
  ): Promise<JsonValue>
}

interface MemosSession {
  readonly id: string
  readonly header: Record<string, JsonValue>
  requestHeader(): { config?: MemosRoute }
}

interface MemosBridge {
  onSessionEvent(session: MemosSession, event: JsonValue): void
  flush(sessionId?: string): Promise<void>
  closeSession(session: MemosSession): Promise<void>
}

export interface MemosAdapterOptions {
  readonly profileId: string
  readonly recallEnabled: boolean
  readonly searchTimeoutMs: number
  readonly bridge: MemosBridge
  readonly runWithLlmRoute?: <T>(route: MemosRoute, operation: () => T) => T
  readonly now?: () => number
}

/** Replay coordinator observations through MemOS's native lifecycle bridge. */
export function createMemosAdapter(
  core: MemosCore,
  options: MemosAdapterOptions,
): MemoryPlugin {
  const now = options.now ?? Date.now
  const routes = new Map<string, MemosRoute>()
  const sessions = new Map<string, MemosSession>()

  const withRoute = <T>(sessionId: string, operation: () => T): T => {
    const route = routes.get(sessionId)
    return route === undefined || options.runWithLlmRoute === undefined
      ? operation()
      : options.runWithLlmRoute(route, operation)
  }

  return {
    id: 'memos',
    filter: goojfcRouteFilters.memos,

    async retrieve(request, context) {
      context.signal?.throwIfAborted()
      const sessionId = sessionIdOf(request)
      rememberRoute(routes, sessionId, request.data)
      if (!options.recallEnabled) return null
      if (request.meta.attributes?.point === 'agent/pre-step'
        && request.meta.attributes.step !== 1) return null
      const query = queryOf(request.data)
      if (query === undefined) throw new TypeError('MemOS retrieval requires textual query data')

      const limit = positiveInteger(recordOf(request.data)?.limit)
      const operation = withRoute(sessionId, () => core.searchMemory({
        agent: 'deepseek-harness',
        namespace: namespaceOf(request, options.profileId, sessionId),
        sessionId,
        query,
        reason: request.meta.attributes?.point === 'agent/pre-step'
          ? 'turn_start'
          : 'tool_driven',
        contextHints: {
          patchouliPoint: stringAttribute(request, 'point') ?? 'external',
          patchouliSource: request.meta.source,
        },
        deadlineAt: now() + options.searchTimeoutMs,
        llmFilterMalformedRetries: 0,
        ...(limit === undefined ? {} : {
          topK: { tier1: limit, tier2: limit, tier3: limit },
        }),
      }, { signal: context.signal, foreground: true }))
      const result = await withHardDeadline(operation, options.searchTimeoutMs)
      context.signal?.throwIfAborted()
      return result
    },

    async update(request, context) {
      context.signal?.throwIfAborted()
      const sessionId = sessionIdOf(request)
      rememberRoute(routes, sessionId, request.data)
      const point = stringAttribute(request, 'point')
      if (point === 'agent/disposed') {
        const session = sessions.get(sessionId)
        if (session !== undefined) await options.bridge.closeSession(session)
        sessions.delete(sessionId)
        routes.delete(sessionId)
        return { closed: true }
      }
      if (point === 'session/turn-end') {
        const session = sessionOf(sessions, routes, sessionId, request.data)
        const events = eventsOf(request.data)
        for (const event of events) options.bridge.onSessionEvent(session, event)
        await options.bridge.flush(sessionId)
        return { stored: true, events: events.length }
      }
      const messages = messagesOf(request.data)
      const userText = messages
        .filter(message => message.role === 'user')
        .map(message => message.content)
        .join('\n\n') || requireContent(request.data, 'MemOS')
      const agentText = messages
        .filter(message => message.role === 'assistant')
        .map(message => message.content)
        .join('\n\n')
      const timestamp = now()
      const namespace = namespaceOf(request, options.profileId, sessionId)
      const prepared = await withRoute(sessionId, () => core.prepareTurn({
        agent: 'deepseek-harness',
        namespace,
        sessionId,
        turnKey: `patchouli:${request.meta.requestId ?? timestamp}`,
        userText,
        contextHints: {
          patchouliPoint: stringAttribute(request, 'point') ?? 'external',
          workspacePath: namespace.workspacePath ?? null,
        },
        ts: timestamp,
      }))
      const result = await withRoute(sessionId, () => core.onTurnEnd({
        agent: 'deepseek-harness',
        namespace,
        sessionId: prepared.sessionId,
        episodeId: prepared.episodeId,
        agentText,
        toolCalls: [],
        contextHints: {
          patchouliPoint: stringAttribute(request, 'point') ?? 'external',
        },
        ts: timestamp,
      }))
      context.signal?.throwIfAborted()
      return result
    },
  }
}

function sessionOf(
  sessions: Map<string, MemosSession>,
  routes: Map<string, MemosRoute>,
  sessionId: string,
  data: JsonValue,
): MemosSession {
  const existing = sessions.get(sessionId)
  if (existing !== undefined) return existing
  const header = recordOf(recordOf(recordOf(data)?.session)?.header)
  const session: MemosSession = {
    id: sessionId,
    header: header === undefined ? { id: sessionId } : { ...header, id: sessionId },
    requestHeader: () => ({ config: routes.get(sessionId) }),
  }
  sessions.set(sessionId, session)
  return session
}

function namespaceOf(
  request: MemoryRetrieveRequest | MemoryUpdateRequest,
  configuredProfileId: string,
  sessionId: string,
): MemosNamespace {
  const sessionHeader = recordOf(recordOf(recordOf(request.data)?.session)?.header)
  const profileId = stringValue(sessionHeader?.agentPreset)
    ?? stringValue(recordOf(request.data)?.profileId)
    ?? configuredProfileId
  const workspacePath = stringAttribute(request, 'workspaceRoot')
  return {
    agentKind: 'deepseek-harness',
    profileId,
    profileLabel: profileId,
    ...(workspacePath === undefined ? {} : { workspacePath }),
    sessionKey: sessionId,
  }
}

function sessionIdOf(request: MemoryRetrieveRequest | MemoryUpdateRequest): string {
  return stringAttribute(request, 'sessionId')
    ?? request.meta.requestId
    ?? `${request.meta.source.type}:${request.meta.source.id}:${request.meta.scope}`
}

function stringAttribute(
  request: MemoryRetrieveRequest | MemoryUpdateRequest,
  key: string,
): string | undefined {
  return stringValue(request.meta.attributes?.[key])
}

function rememberRoute(routes: Map<string, MemosRoute>, sessionId: string, data: JsonValue): void {
  const route = recordOf(recordOf(recordOf(data)?.agent)?.options)
  const provider = stringValue(route?.provider)
  const model = stringValue(route?.model)
  if (provider === undefined || model === undefined) return
  const reasoningEffort = stringValue(route?.reasoningEffort)
  routes.set(sessionId, {
    provider,
    model,
    ...(reasoningEffort === undefined ? {} : { reasoningEffort }),
    sessionId,
  })
}

function positiveInteger(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0
    ? value
    : undefined
}

async function withHardDeadline<T>(operation: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: NodeJS.Timeout | undefined
  const timeout = new Promise<never>((_resolve, reject) => {
    timer = setTimeout(() => reject(new Error(`MemOS search timed out after ${timeoutMs}ms`)), timeoutMs)
    timer.unref()
  })
  try {
    return await Promise.race([operation, timeout])
  } finally {
    if (timer !== undefined) clearTimeout(timer)
  }
}
