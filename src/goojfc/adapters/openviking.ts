import type {
  MemoryData,
  MemoryPlugin,
  MemoryPluginContext,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  eventsOf,
  messagesOf,
  queryOf,
  recordOf,
  stringValue,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

interface OpenVikingSession {
  readonly id: string
  readonly header: { readonly cwd: string }
}

interface OpenVikingAgent {
  readonly session: OpenVikingSession
}

export interface OpenVikingRuntime {
  initialize(agent: OpenVikingAgent): Promise<unknown>
  stateFor(session: OpenVikingSession): {
    readonly config: { readonly peerId?: string }
  }
  capture(session: OpenVikingSession, event: unknown): void
  maybeCommit(session: OpenVikingSession, event: unknown): void
  flush(session: OpenVikingSession): Promise<void>
  dispose(session: OpenVikingSession): Promise<void> | void
  profileMessage(agent: OpenVikingAgent): Promise<MemoryData | null>
  recallMessage(agent: OpenVikingAgent, messages: readonly unknown[]): Promise<MemoryData | null>
}

export interface OpenVikingClient {
  addResource(path: string, reason?: string, actorPeerId?: string): Promise<MemoryData | null>
}

/** Add explicit calls through OpenViking's native queued runtime. */
export function createOpenVikingAdapter(
  runtime: OpenVikingRuntime,
  client: OpenVikingClient,
): MemoryPlugin {
  return {
    id: 'openviking',
    filter: goojfcRouteFilters.openviking,
    async update(request, context) {
      return updateOpenViking(runtime, client, request, context)
    },
    async retrieve(request, context) {
      return retrieveOpenViking(runtime, request, context)
    },
  }
}

async function updateOpenViking(
  runtime: OpenVikingRuntime,
  client: OpenVikingClient,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const agent = agentOf(request)
  const point = request.meta.attributes?.point
  if (point === 'agent/disposed') {
    await runtime.dispose(agent.session)
    return { disposed: true }
  }
  if (point === 'agent/created') {
    await runtime.initialize(agent)
    return { initialized: true }
  }
  const data = recordOf(request.data)
  const events = eventsOf(request.data)
  const messages = messagesOf(request.data)
  const resources = Array.isArray(data?.resources) ? data.resources : []
  if (events.length === 0 && messages.length === 0 && resources.length === 0) {
    throw new TypeError('OpenViking update requires events, textual messages, or resources')
  }

  await runtime.initialize(agent)
  for (const event of events) {
    runtime.capture(agent.session, event)
    runtime.maybeCommit(agent.session, event)
  }
  for (const message of messages) {
    runtime.capture(agent.session, eventOf(message.role, message.content))
  }
  if (events.length > 0 || messages.length > 0) await runtime.flush(agent.session)

  const results: MemoryData[] = []
  const actorPeerId = runtime.stateFor(agent.session).config.peerId
  for (const value of resources) {
    const resource = recordOf(value)
    const path = stringValue(resource?.path)
    if (path === undefined) throw new TypeError('OpenViking resources require a path')
    context.signal?.throwIfAborted()
    const result = await client.addResource(path, stringValue(resource?.reason), actorPeerId)
    if (result === null) throw new Error('OpenViking add resource failed')
    results.push(result)
  }
  context.signal?.throwIfAborted()
  return {
    captured: events.length + messages.length,
    resources: results,
  }
}

async function retrieveOpenViking(
  runtime: OpenVikingRuntime,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const query = queryOf(request.data)
  const agent = agentOf(request)
  if (request.meta.attributes?.point === 'agent/session-start') {
    const profile = await runtime.profileMessage(agent)
    context.signal?.throwIfAborted()
    return profile ?? null
  }
  if (query === undefined) throw new TypeError('OpenViking retrieval requires textual query data')
  const message = await runtime.recallMessage(agent, [messageOf('user', query)])
  context.signal?.throwIfAborted()
  return message ?? null
}

function agentOf(
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
): OpenVikingAgent {
  const configuredRoot = request.meta.attributes?.workspaceRoot
  const root = typeof configuredRoot === 'string' && configuredRoot.trim() !== ''
    ? configuredRoot
    : process.cwd()
  const sessionId = request.meta.attributes?.sessionId
  return {
    session: {
      id: typeof sessionId === 'string' && sessionId.trim() !== ''
        ? sessionId
        : `${request.meta.source.type}:${request.meta.source.id}:${request.meta.scope}`,
      header: { cwd: root },
    },
  }
}

function eventOf(role: 'user' | 'assistant', content: string): MemoryData {
  const message = messageOf(role, content)
  return role === 'assistant'
    ? { type: 'assistant/message', data: { message } }
    : { type: 'user/message', data: message }
}

function messageOf(role: 'user' | 'assistant', content: string): MemoryData {
  return {
    role,
    content: [{ type: 'text', text: content }],
    source: { kind: role === 'user' ? 'user' : 'assistant' },
  }
}
