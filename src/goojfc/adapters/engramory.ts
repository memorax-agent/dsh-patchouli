import {
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from 'node:fs'
import { createHash, randomUUID } from 'node:crypto'
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path'

import type {
  MemoryData,
  MemoryPlugin,
  MemoryPluginContext,
  MemoryRetrieveRequest,
  MemoryUpdateRequest,
} from '../../memory.js'
import {
  contentText,
  recordOf,
  requireContent,
  stringValue,
} from './input.js'
import { goojfcRouteFilters } from '../routing.js'

export interface EngramoryAdapterOptions {
  readonly memoryRoot: string
  readonly indexName: string
  readonly validateIndex: (content: string, path: string) => string | undefined
}

/** Complete Engramory's guard-only plugin with its documented Markdown store protocol. */
export function createEngramoryAdapter(options: EngramoryAdapterOptions): MemoryPlugin {
  const root = resolve(options.memoryRoot)
  let writes = Promise.resolve()
  return {
    id: 'engramory',
    filter: goojfcRouteFilters.engramory,
    async update(request, context) {
      const operation = writes.then(() => updateEngramory(options, root, request, context))
      writes = operation.then(() => undefined, () => undefined)
      return operation
    },
    async retrieve(request, context) {
      await writes
      return retrieveEngramory(options, root, request, context)
    },
  }
}

async function updateEngramory(
  options: EngramoryAdapterOptions,
  root: string,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const data = recordOf(request.data)
  const content = requireContent(request.data, 'Engramory')

  const indexPath = join(root, basename(options.indexName))
  const name = slug(
    stringOf(data?.name)
      ?? stringOf(data?.title)
      ?? request.meta.requestId
      ?? `patchouli-${createHash('sha256').update(content).digest('hex').slice(0, 12)}`,
  )
  const noteName = `${name}.md`
  const notePath = join(root, noteName)
  const description = stringOf(data?.description) ?? content.slice(0, 160).replace(/\s+/g, ' ')
  const type = stringOf(data?.type) ?? 'reference'
  const scope = stringOf(data?.scope) ?? 'repo'
  const date = new Date().toISOString().slice(0, 10)
  const created = readSafeText(root, notePath)?.match(/^created:\s*(\d{4}-\d{2}-\d{2})\s*$/m)?.[1]
    ?? date
  const note = [
    '---',
    `name: ${name}`,
    `description: ${description.replace(/[\r\n]/g, ' ')}`,
    `type: ${type.replace(/[\r\n]/g, ' ')}`,
    `scope: ${scope.replace(/[\r\n]/g, ' ')}`,
    `created: ${created}`,
    `updated: ${date}`,
    '---',
    '',
    content,
    '',
  ].join('\n')
  const currentIndex = readSafeText(root, indexPath) ?? ''
  const pointer = `- ${description.replace(/[\r\n]/g, ' ')} [${name}](${noteName})`
  const pointerPattern = new RegExp(`^.*\\]\\(${escapeRegExp(noteName)}\\)\\s*$`, 'm')
  const nextIndex = pointerPattern.test(currentIndex)
    ? currentIndex.replace(pointerPattern, pointer)
    : `${currentIndex.trimEnd()}${currentIndex.trim() === '' ? '' : '\n'}${pointer}\n`
  const refusal = options.validateIndex(nextIndex, indexPath)
  if (refusal !== undefined) throw new Error(refusal)

  mkdirSync(root, { recursive: true })
  atomicWrite(notePath, note)
  atomicWrite(indexPath, nextIndex)
  context.signal?.throwIfAborted()
  return { indexPath, notePath, pointer }
}

async function retrieveEngramory(
  options: EngramoryAdapterOptions,
  root: string,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const indexPath = join(root, basename(options.indexName))
  const data = recordOf(request.data)
  const query = realUserQuery(request.data)
  if (query === undefined) return null
  const limit = positiveLimit(data?.limit, 8)
  const index = readSafeText(root, indexPath)
  if (index === undefined) return null
  const lines: string[] = []
  const notes: MemoryData[] = []
  for (const line of index.split('\n')) {
    if (lines.length >= limit) break
    const match = line.match(/\[[^\]]+\]\(([^)]+\.md)\)/)
    if (match?.[1] === undefined) continue
    const notePath = safeChild(root, match[1])
    if (notePath === undefined) continue
    const content = readSafeText(root, notePath)
    if (content === undefined || content === '') continue
    if (!matchesQuery(line, query) && !matchesQuery(content, query)) continue
    lines.push(line)
    notes.push({ path: notePath, content })
  }
  context.signal?.throwIfAborted()
  return notes.length === 0 ? null : { index: lines, notes }
}

function safeChild(root: string, candidate: string): string | undefined {
  if (isAbsolute(candidate)) return undefined
  const path = resolve(root, candidate)
  const child = relative(root, path)
  return child !== '' && !child.startsWith('..') && !isAbsolute(child) ? path : undefined
}

function atomicWrite(path: string, content: string): void {
  mkdirSync(dirname(path), { recursive: true })
  const temporary = `${path}.patchouli-${process.pid}-${randomUUID()}.tmp`
  writeFileSync(temporary, content, 'utf8')
  renameSync(temporary, path)
}

function readSafeText(root: string, path: string): string | undefined {
  try {
    const canonicalRoot = realpathSync(root)
    const stat = lstatSync(path)
    if (!stat.isFile() || stat.isSymbolicLink()) return undefined
    const canonicalPath = realpathSync(path)
    const child = relative(canonicalRoot, canonicalPath)
    if (child === '' || child.startsWith('..') || isAbsolute(child)) return undefined
    return readFileSync(canonicalPath, 'utf8')
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined
    throw error
  }
}

function realUserQuery(data: MemoryData): string | undefined {
  if (typeof data === 'string') return stringValue(data)
  const record = recordOf(data)
  const direct = stringValue(record?.query)
  if (direct !== undefined) return direct
  if (!Array.isArray(record?.messages)) return undefined
  for (let index = record.messages.length - 1; index >= 0; index -= 1) {
    const message = recordOf(record.messages[index])
    const source = recordOf(message?.source)
    if (source?.kind !== 'user') continue
    const content = contentText(message?.content)
    if (content !== '') return content
  }
  return undefined
}

function matchesQuery(line: string, query: string): boolean {
  const haystack = line.toLocaleLowerCase()
  const normalized = query.toLocaleLowerCase().trim()
  if (haystack.includes(normalized)) return true
  const terms = normalized.match(/[\p{L}\p{N}_-]+/gu) ?? []
  return terms.some(term => term.length >= 3 && haystack.includes(term))
}

function positiveLimit(value: unknown, fallback: number): number {
  if (value === undefined) return fallback
  if (Number.isSafeInteger(value) && typeof value === 'number' && value > 0) return value
  throw new TypeError('Engramory limit must be a positive safe integer')
}

function slug(value: string): string {
  const result = value.trim().toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fff]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80)
  return result === '' ? 'patchouli-memory' : result
}

function stringOf(value: unknown): string | undefined {
  return stringValue(value)
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}
