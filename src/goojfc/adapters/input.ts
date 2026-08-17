import type { JsonValue } from '@memorax-agent/patchouli-protocol'

import type { MemoryData, MemoryRouteCall } from '../../memory.js'

const toolPoint: Readonly<Record<'update' | 'retrieve', string>> = {
  update: 'tool/memory-update',
  retrieve: 'tool/memory-retrieve',
}

export function supportsAgentLoopPoints(options: {
  readonly update?: readonly string[]
  readonly retrieve?: readonly string[]
  readonly allowToolUpdate?: boolean
}): (call: MemoryRouteCall) => boolean {
  const update = new Set(options.update)
  const retrieve = new Set(options.retrieve)
  return (call) => {
    const point = stringValue(call.meta.attributes?.point)
    if (point === undefined) return call.operation !== 'subscribe'
    if (call.operation !== 'subscribe' && point === toolPoint[call.operation]) {
      return call.operation !== 'update' || options.allowToolUpdate !== false
    }
    return call.operation === 'update'
      ? update.has(point)
      : call.operation === 'retrieve' && retrieve.has(point)
  }
}

export function recordOf(value: unknown): Record<string, JsonValue> | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, JsonValue>
    : undefined
}

export function stringValue(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() !== '' ? value.trim() : undefined
}

export function stringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined
  const result = value.flatMap(item => {
    const text = stringValue(item)
    return text === undefined ? [] : [text]
  })
  return result.length === 0 ? undefined : result
}

export function messagesOf(data: MemoryData): Array<{ role: 'user' | 'assistant'; content: string }> {
  const messages = recordOf(data)?.messages
  if (!Array.isArray(messages)) return []
  return messages.flatMap((value) => {
    const message = recordOf(value)
    if (message === undefined) return []
    const content = contentText(message.content)
    if (content === '') return []
    return [{
      role: message.role === 'assistant' ? 'assistant' : 'user',
      content,
    }]
  })
}

export function eventsOf(data: MemoryData): readonly JsonValue[] {
  const record = recordOf(data)
  if (Array.isArray(record?.events)) return record.events
  const session = recordOf(record?.session)
  return Array.isArray(session?.events) ? session.events : []
}

export function eventMessagesOf(
  data: MemoryData,
): Array<{ role: 'user' | 'assistant'; content: string }> {
  return eventsOf(data).flatMap((value) => {
    const event = recordOf(value)
    if (event === undefined) return []
    const message = event.type === 'assistant/message'
      ? recordOf(recordOf(event.data)?.message)
      : event.type === 'user/message' ? recordOf(event.data) : undefined
    if (message === undefined) return []
    const content = contentText(message.content)
    if (content === '') return []
    return [{
      role: event.type === 'assistant/message' ? 'assistant' : 'user',
      content,
    }]
  })
}

export function contentOf(data: MemoryData): string | undefined {
  if (typeof data === 'string') return stringValue(data)
  const direct = stringValue(recordOf(data)?.content)
  if (direct !== undefined) return direct
  const messages = messagesOf(data)
  if (messages.length === 0) return undefined
  return messages.map(message => message.content).join('\n\n')
}

export function queryOf(data: MemoryData): string | undefined {
  if (typeof data === 'string') return stringValue(data)
  const direct = stringValue(recordOf(data)?.query)
  if (direct !== undefined) return direct
  const messages = recordOf(data)?.messages
  if (!Array.isArray(messages)) return undefined
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = recordOf(messages[index])
    if (message?.role !== 'user' || recordOf(message.source)?.kind !== 'user') continue
    const content = contentText(message.content)
    if (content !== '') return content
  }
  return undefined
}

export function requireContent(data: MemoryData, plugin: string): string {
  const content = contentOf(data)
  if (content === undefined) {
    throw new TypeError(`${plugin} update requires content or textual messages`)
  }
  return content
}

export function requireQuery(data: MemoryData, plugin: string): string {
  const query = queryOf(data)
  if (query === undefined) {
    throw new TypeError(`${plugin} retrieval requires a query or textual user messages`)
  }
  return query
}

export function contentText(value: JsonValue | undefined): string {
  const direct = stringValue(value)
  if (direct !== undefined) return direct
  if (!Array.isArray(value)) return ''
  return value.flatMap((block) => {
    const record = recordOf(block)
    const text = record?.type === 'text' ? stringValue(record.text) : undefined
    return text === undefined ? [] : [text]
  }).join('\n\n')
}
