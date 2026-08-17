import type {
  MemoryData,
  MemoryPlugin,
  MemoryPluginContext,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  recordOf,
  requireContent,
  requireQuery,
  stringArray,
  stringValue,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'
import { createHash } from 'node:crypto'

const memoryTypes = new Set(['preference', 'project', 'decision', 'history', 'summary'])

export interface MnemeService {
  saveWithDedupe(memory: Record<string, unknown>): MemoryData
  searchMemories(
    query: string,
    options: Record<string, unknown>,
  ): Promise<readonly MemoryData[]>
  toApiList(rows: readonly MemoryData[]): MemoryData[]
  injectCandidates(options: {
    readonly maxItems: number
    readonly threshold: number
  }): readonly MemoryData[]
}

export interface MnemeSession {
  readonly id: string
  readonly events: readonly MemoryData[]
  requestHeader(): { config?: Record<string, unknown> }
}

export interface MnemeSummarizer {
  summarize?(session: MnemeSession): Promise<void>
}

export interface MnemeAdapterOptions {
  readonly autoInject: boolean
  readonly maxInjectedItems: number
  readonly importanceThreshold: number
  readonly session: (sessionId: string) => MnemeSession | undefined
  readonly getProfile: () => string
  readonly getRules: () => readonly string[]
}

/** Route generic Patchouli calls through Mneme's native service. */
export function createMnemeAdapter(
  service: MnemeService,
  summarizer: MnemeSummarizer,
  options: MnemeAdapterOptions,
): MemoryPlugin {
  return {
    id: 'mneme',
    filter: goojfcRouteFilters.mneme,
    async update(request, context) {
      return updateMneme(service, summarizer, options, request, context)
    },
    async retrieve(request, context) {
      return retrieveMneme(service, options, request, context)
    },
  }
}

async function updateMneme(
  service: MnemeService,
  summarizer: MnemeSummarizer,
  options: MnemeAdapterOptions,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  if (request.meta.attributes?.point === 'session/turn-end') {
    if (summarizer.summarize === undefined) return { summarized: false }
    const sessionId = stringValue(request.meta.attributes.sessionId)
      ?? `${request.meta.source.type}:${request.meta.source.id}`
    const session = options.session(sessionId)
    if (session === undefined) throw new Error(`Mneme session ${JSON.stringify(sessionId)} is not live`)
    await summarizer.summarize(session)
    context.signal?.throwIfAborted()
    return { summarized: true }
  }
  const data = recordOf(request.data)
  const content = requireContent(request.data, 'Mneme')

  const result = service.saveWithDedupe({
    type: memoryTypes.has(String(data?.type)) ? data?.type : 'history',
    title: stringValue(data?.title)
      ?? request.meta.requestId
      ?? `patchouli:${createHash('sha256').update(content).digest('hex').slice(0, 24)}`,
    content,
    tags: stringArray(data?.tags) ?? [],
    importance: Number.isInteger(data?.importance)
      && Number(data?.importance) >= 1
      && Number(data?.importance) <= 5
      ? data?.importance
      : 3,
    source: 'patchouli',
  })
  context.signal?.throwIfAborted()
  return result
}

async function retrieveMneme(
  service: MnemeService,
  options: MnemeAdapterOptions,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  if (request.meta.attributes?.point === 'agent/pre-step') {
    if (!options.autoInject) return null
    const injection = renderInjection(service, options)
    return injection === '' ? null : injection
  }
  const data = recordOf(request.data)
  const query = requireQuery(request.data, 'Mneme')
  const rows = await service.searchMemories(query, {
    topK: Number.isInteger(data?.limit) && Number(data?.limit) > 0
      ? data?.limit
      : 20,
    mode: typeof data?.mode === 'string' ? data.mode : 'auto',
    recordRecall: true,
  })
  context.signal?.throwIfAborted()
  const items = service.toApiList(rows)
  return items.length === 0 ? null : { items }
}

function renderInjection(service: MnemeService, options: MnemeAdapterOptions): string {
  const sections: string[] = []
  const profile = options.getProfile().trim()
  const rules = options.getRules().map(rule => rule.trim()).filter(Boolean)
  if (profile !== '' || rules.length > 0) {
    const lines = ['[用户设置] 来自 dsh-mneme 的用户画像与规则：']
    if (profile !== '') lines.push(`- 用户画像：${profile}`)
    for (const rule of rules) lines.push(`- 规则：${rule}`)
    sections.push(lines.join('\n'))
  }

  const candidates = service.injectCandidates({
    maxItems: options.maxInjectedItems,
    threshold: options.importanceThreshold,
  })
  if (candidates.length > 0) {
    const lines = ['[记忆库] 来自 dsh-mneme 的跨会话记忆（用户偏好与高优先级项目/决策）：']
    for (const value of candidates) {
      const memory = recordOf(value)
      if (memory === undefined) continue
      lines.push(`- [${String(memory.type)}] ${String(memory.title)}（重要性 ${String(memory.importance)}）：${String(memory.content)}`)
    }
    if (lines.length > 1) sections.push(lines.join('\n'))
  }
  return sections.join('\n\n')
}
