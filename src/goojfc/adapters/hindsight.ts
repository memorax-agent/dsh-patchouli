import type { JsonValue } from 'dsh-patchouli-protocol'
import { createHash } from 'node:crypto'

import type {
  MemoryPlugin,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  eventMessagesOf,
  messagesOf,
  queryOf,
  recordOf,
  requireContent,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

interface HindsightClient {
  retain(
    content: string,
    context: string,
    documentId: string,
    tags: string[],
    strategy: 'conversation' | 'document',
    options: { metadata: Record<string, string>, operationId: string },
  ): Promise<void>
  reflect(
    query: string,
    options: { budget: 'low', timeoutMs: number },
  ): Promise<string>
}

interface HindsightWorkspace {
  readonly root: string
  readonly core: {
    readonly bankId: string
    readonly cfg: {
      readonly reflectTimeoutMs: number
      readonly [key: string]: unknown
    }
    readonly client: HindsightClient
    onPrompt?(sessionId: string, prompt: string): Promise<void>
    getInjection?(sessionId: string): string | undefined
  }
}

interface RetainStamp {
  readonly tags: string[]
  readonly metadata: Record<string, string>
}

export interface HindsightAdapterNative {
  readonly harness: string
  readonly maxReflectTimeoutMs: number
  workspaceFor(root: string): HindsightWorkspace | undefined
  ensureSeeded(workspace: HindsightWorkspace): void
  retainTranscript(
    workspace: HindsightWorkspace,
    sessionId: string,
    throughSeq: number,
  ): Promise<void>
  readEvents(events: readonly unknown[]): Array<{ role: string, content: string }>
  retainStamp(workspace: HindsightWorkspace, sessionId?: string): RetainStamp
  reflectQuery(query: string): string
  operationId(value: string): string
}

/** Map Patchouli's generic envelope onto Hindsight's existing bank client. */
export function createHindsightAdapter(native: HindsightAdapterNative): MemoryPlugin {
  return {
    id: 'hindsight',
    filter: goojfcRouteFilters.hindsight,

    async update(request, context) {
      context.signal?.throwIfAborted()
      const workspace = workspaceOf(native, request)
      const sessionId = stringAttribute(request, 'sessionId')
      if (request.meta.attributes?.point === 'session/turn-end') {
        if (sessionId === undefined) {
          throw new TypeError('Hindsight session capture requires meta.attributes.sessionId')
        }
        const event = recordOf(recordOf(request.data)?.event)
        const endSeq = event?.seq
        if (typeof endSeq !== 'number' || !Number.isSafeInteger(endSeq) || endSeq < 0) {
          throw new TypeError('Hindsight session capture requires data.event.seq')
        }
        await native.retainTranscript(workspace, sessionId, endSeq)
        context.signal?.throwIfAborted()
        return {
          accepted: true,
          bankId: workspace.core.bankId,
          documentId: `conversation:${sessionId}`,
        }
      }

      const turns = turnsOf(native, request.data)
      const content = turns.length > 0
        ? turns.map(turn => JSON.stringify(turn)).join('\n')
        : requireContent(request.data, 'Hindsight')

      const documentId = `patchouli:${request.meta.requestId ?? contentId(content)}`
      const stamp = native.retainStamp(workspace, sessionId)
      const conversation = turns.length > 0
      const metadata = {
        ...stamp.metadata,
        source: 'patchouli',
        source_type: request.meta.source.type,
        source_id: request.meta.source.id,
        scope: request.meta.scope,
      }
      await workspace.core.client.retain(
        content,
        conversation ? 'coding agent session' : 'patchouli memory update',
        documentId,
        [...new Set([
          ...stamp.tags,
          conversation ? 'source:chat' : 'source:upload',
          `harness:${native.harness}`,
        ])],
        conversation ? 'conversation' : 'document',
        {
          metadata,
          operationId: native.operationId(
            `${workspace.core.bankId}\n${documentId}\n${content}`,
          ),
        },
      )
      context.signal?.throwIfAborted()
      return {
        accepted: true,
        bankId: workspace.core.bankId,
        documentId,
      } as JsonValue
    },

    async retrieve(request, context) {
      context.signal?.throwIfAborted()
      const query = queryOf(request.data)
      if (query === undefined) throw new TypeError('Hindsight retrieval requires textual query data')

      const workspace = workspaceOf(native, request)
      const sessionId = stringAttribute(request, 'sessionId')
      if (request.meta.attributes?.point === 'agent/pre-step'
        && sessionId !== undefined
        && workspace.core.onPrompt !== undefined
        && workspace.core.getInjection !== undefined) {
        await workspace.core.onPrompt(sessionId, query)
        context.signal?.throwIfAborted()
        return { text: workspace.core.getInjection(sessionId) ?? '' }
      }
      const text = await workspace.core.client.reflect(native.reflectQuery(query), {
        budget: 'low',
        timeoutMs: Math.min(
          workspace.core.cfg.reflectTimeoutMs,
          native.maxReflectTimeoutMs,
        ),
      })
      context.signal?.throwIfAborted()
      return { text }
    },
  }
}

function workspaceOf(
  native: HindsightAdapterNative,
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
): HindsightWorkspace {
  const root = stringAttribute(request, 'workspaceRoot') ?? request.meta.scope
  const workspace = native.workspaceFor(root)
  if (workspace === undefined) throw new Error('Hindsight is disabled for this workspace')
  native.ensureSeeded(workspace)
  return workspace
}

function turnsOf(
  native: HindsightAdapterNative,
  data: JsonValue,
): Array<{ role: string, content: string }> {
  const record = recordOf(data)
  if (Array.isArray(record?.events)) return native.readEvents(record.events)
  const eventMessages = eventMessagesOf(data)
  if (eventMessages.length > 0) return eventMessages
  return messagesOf(data)
}

function stringAttribute(
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
  name: string,
): string | undefined {
  const value = request.meta.attributes?.[name]
  return typeof value === 'string' && value.trim() !== '' ? value : undefined
}

function contentId(content: string): string {
  return `content:${createHash('sha256').update(content).digest('hex').slice(0, 24)}`
}
