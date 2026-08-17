import type {
  MemoryData,
  MemoryPlugin,
  MemoryPluginContext,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  queryOf,
  recordOf,
  requireContent,
  requireQuery,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

export interface MnemonService {
  remember(
    request: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<MemoryData>
  search(
    request: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<MemoryData>
}

export interface MnemonRuntime {
  readonly config: {
    readonly storageScope?: string
    readonly lifecycleEnabled?: boolean
    readonly recallMode?: string
  }
  readonly service: MnemonService
  forWorkspacePath(path: string): { readonly service: MnemonService }
}

export interface MnemonLifecycle {
  remember(sessionId: string, request: Record<string, unknown>, signal?: AbortSignal): Promise<MemoryData>
  recall(sessionId: string, request: Record<string, unknown>, signal?: AbortSignal): Promise<MemoryData>
}

export interface MnemonSessionContext {
  readonly root: boolean
  readonly hotMemory: string
}

export interface MnemonAdapterOptions {
  readonly session: (sessionId: string) => MnemonSessionContext | undefined
}

/** Route generic Patchouli calls through Mnemon's live native runtime. */
export function createMnemonAdapter(
  runtime: MnemonRuntime,
  lifecycle: MnemonLifecycle,
  options: MnemonAdapterOptions,
): MemoryPlugin {
  return {
    id: 'mnemon',
    filter: goojfcRouteFilters.mnemon,
    async update(request, context) {
      return updateMnemon(runtime, lifecycle, options, request, context)
    },
    async retrieve(request, context) {
      return retrieveMnemon(runtime, lifecycle, options, request, context)
    },
  }
}

async function updateMnemon(
  runtime: MnemonRuntime,
  lifecycle: MnemonLifecycle,
  options: MnemonAdapterOptions,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const data = recordOf(request.data) ?? {}
  const content = requireContent(request.data, 'Mnemon')
  const input = { ...data, content }
  const sessionId = sessionIdForTool(request)
  const session = sessionId === undefined ? undefined : options.session(sessionId)
  if (sessionId !== undefined && session === undefined) {
    throw new Error(`Mnemon session ${JSON.stringify(sessionId)} is not live`)
  }
  const result = sessionId === undefined || session?.root !== true
    ? await serviceFor(runtime, request).remember(input, context.signal)
    : await lifecycle.remember(sessionId, input, context.signal)
  context.signal?.throwIfAborted()
  return result
}

async function retrieveMnemon(
  runtime: MnemonRuntime,
  lifecycle: MnemonLifecycle,
  options: MnemonAdapterOptions,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  if (request.meta.attributes?.point === 'agent/pre-step') {
    const sessionId = requiredSessionId(request)
    const session = options.session(sessionId)
    if (session === undefined) throw new Error(`Mnemon session ${JSON.stringify(sessionId)} is not live`)
    const hotMemory = session.hotMemory.trim()
    const recallEnabled = session.root
      && request.meta.attributes.step === 1
      && runtime.config.lifecycleEnabled !== false
      && runtime.config.recallMode !== 'off'
    if (!recallEnabled) return hotMemory === '' ? null : { hotMemory }

    const query = queryOf(request.data)
    if (query === undefined) return hotMemory === '' ? null : { hotMemory }
    const recall = normalizeRetrieveResult(await lifecycle.recall(
      sessionId,
      { query },
      context.signal,
    ))
    context.signal?.throwIfAborted()
    if (hotMemory === '') return recall
    return recall === null ? { hotMemory } : { hotMemory, recall }
  }
  const data = recordOf(request.data) ?? {}
  const query = requireQuery(request.data, 'Mnemon')
  const input = { ...data, query }
  const sessionId = sessionIdForTool(request)
  const session = sessionId === undefined ? undefined : options.session(sessionId)
  if (sessionId !== undefined && session === undefined) {
    throw new Error(`Mnemon session ${JSON.stringify(sessionId)} is not live`)
  }
  const result = sessionId === undefined || session?.root !== true
    ? await serviceFor(runtime, request).search(input, context.signal)
    : await lifecycle.recall(sessionId, input, context.signal)
  context.signal?.throwIfAborted()
  return normalizeRetrieveResult(result)
}

function sessionIdForTool(
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
): string | undefined {
  const point = request.meta.attributes?.point
  if (point !== 'tool/memory-update'
    && point !== 'tool/memory-retrieve') return undefined
  return requiredSessionId(request)
}

function requiredSessionId(
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
): string {
  const sessionId = request.meta.attributes?.sessionId
  if (typeof sessionId !== 'string' || sessionId.trim() === '') {
    throw new TypeError('Mnemon coordinated calls require meta.attributes.sessionId')
  }
  return sessionId
}

function normalizeRetrieveResult(result: MemoryData): MemoryData {
  if (result === null) return null
  const value = recordOf(result)
  return Array.isArray(value?.results) && value.results.length === 0 ? null : result
}

function serviceFor(
  runtime: MnemonRuntime,
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
): MnemonService {
  if (runtime.config.storageScope !== 'workspace') return runtime.service
  const workspaceRoot = request.meta.attributes?.workspaceRoot
  if (typeof workspaceRoot !== 'string' || workspaceRoot.trim() === '') {
    throw new TypeError('Mnemon workspace storage requires meta.attributes.workspaceRoot')
  }
  return runtime.forWorkspacePath(
    workspaceRoot,
  ).service
}
