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
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

interface LingshuToolResult {
  readonly content?: MemoryData
  readonly result?: MemoryData
  readonly isError?: boolean
}

export interface LingshuBridge {
  callTool(name: string, args: Record<string, unknown>): Promise<MemoryData>
}

export interface LingshuCaptureConfig {
  readonly userMessage: boolean
  readonly assistantMessage: boolean
  readonly toolResult: boolean
  readonly importance: number
}

/** Add explicit Patchouli calls through the plugin's live AEIS MCP bridge. */
export function createLingshuAdapter(
  bridge: LingshuBridge,
  capture: LingshuCaptureConfig,
): MemoryPlugin {
  return {
    id: 'lingshu',
    filter: goojfcRouteFilters.lingshu,
    async update(request, context) {
      if (request.meta.attributes?.point === 'session/turn-end') {
        return captureTurn(bridge, capture, request, context)
      }
      return callUpdate(bridge, request, context)
    },
    async retrieve(request, context) {
      return callRetrieve(bridge, request, context)
    },
  }
}

async function captureTurn(
  bridge: LingshuBridge,
  capture: LingshuCaptureConfig,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  const stored: MemoryData[] = []
  for (const value of eventsOf(request.data)) {
    const event = recordOf(value)
    const type = event?.type
    const data = recordOf(event?.data)
    let text = ''
    let importance = capture.importance
    let tag = ''
    if (type === 'user/message' && capture.userMessage
      && recordOf(data?.source)?.kind === 'user') {
      text = contentText(data?.content)
      tag = 'user'
    } else if (type === 'assistant/message' && capture.assistantMessage) {
      text = contentText(recordOf(data?.message)?.content)
      importance *= 0.8
      tag = 'assistant'
    } else if (type === 'tool/result' && capture.toolResult && !data?.error) {
      text = contentText(recordOf(data?.message)?.content)
      importance *= 0.6
      tag = 'tool'
    }
    if (text === '') continue
    context.signal?.throwIfAborted()
    stored.push(unwrapResult(await bridge.callTool('remember', {
      content: text,
      importance,
      tags: ['dsh', tag],
    }), 'remember'))
  }
  return { stored }
}

async function callUpdate(
  bridge: LingshuBridge,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const data = recordOf(request.data) ?? {}
  const result = await bridge.callTool('remember', {
    ...data,
    content: requireContent(request.data, 'Lingshu'),
  })
  context.signal?.throwIfAborted()
  return unwrapResult(result, 'remember')
}

async function callRetrieve(
  bridge: LingshuBridge,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const data = recordOf(request.data) ?? {}
  const tool = data.tool === 'search' ? 'search' : 'recall'
  const { tool: _tool, query: _query, ...args } = data
  const automatic = request.meta.attributes?.point === 'agent/pre-step'
  const query = automatic ? queryOf(request.data) : requireQuery(request.data, 'Lingshu')
  if (query === undefined) return null
  const result = await bridge.callTool(tool, {
    ...automatic ? {} : args,
    query,
  })
  context.signal?.throwIfAborted()
  return unwrapResult(result, tool, true)
}

function unwrapResult(value: MemoryData, tool: string, emptyAsNull = false): MemoryData {
  const result = recordOf(value) as LingshuToolResult | undefined
  if (result?.isError === true) {
    throw new Error(textOf(result.content) || `Lingshu ${tool} failed`)
  }
  if (result?.result !== undefined) return emptyAsNull && isEmpty(result.result) ? null : result.result
  if (result?.content !== undefined) {
    if (emptyAsNull && isEmpty(result.content)) return null
    const text = textOf(result.content)
    return text === '' ? { content: result.content } : text
  }
  return value
}

function isEmpty(value: MemoryData): boolean {
  if (value === null || value === '') return true
  if (Array.isArray(value)) return value.length === 0
  const result = recordOf(value)
  return Array.isArray(result?.items) && result.items.length === 0
    || Array.isArray(result?.results) && result.results.length === 0
}

function textOf(value: MemoryData | undefined): string {
  if (typeof value === 'string') return value
  if (!Array.isArray(value)) return ''
  return value.flatMap((block) => {
    const record = recordOf(block)
    return record?.type === 'text' && typeof record.text === 'string' ? [record.text] : []
  }).join('\n')
}
