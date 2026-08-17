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

type GraphNodeType = 'TASK' | 'SKILL' | 'EVENT'

interface GraphNodeInput {
  readonly type: GraphNodeType
  readonly name: string
  readonly description: string
  readonly content: string
}

interface GraphEdgeInput {
  readonly from: string
  readonly to: string
  readonly type: string
  readonly instruction?: string
  readonly condition?: string
}

export interface GraphMemoryNative {
  recall(query: string): Promise<MemoryData>
  upsertNode(node: GraphNodeInput, sessionId: string): MemoryData
  upsertEdge(edge: GraphEdgeInput, sessionId: string): MemoryData
}

const nodeTypes = new Set<GraphNodeType>(['TASK', 'SKILL', 'EVENT'])

/** Expose explicit graph CRUD plus recall; native OpenClaw extraction is not emulated. */
export function createGraphMemoryAdapter(native: GraphMemoryNative): MemoryPlugin {
  return {
    id: 'graph-memory',
    filter: goojfcRouteFilters['graph-memory'],
    async update(request, context) {
      return updateGraph(native, request, context)
    },
    async retrieve(request, context) {
      return retrieveGraph(native, request, context)
    },
  }
}

async function updateGraph(
  native: GraphMemoryNative,
  request: MemoryUpdateRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const data = recordOf(request.data)
  if (data === undefined) {
    throw new TypeError('graph-memory update requires structured nodes and optional edges')
  }
  const nodeValues = Array.isArray(data.nodes) ? data.nodes : isNode(data) ? [data] : []
  const edgeValues = Array.isArray(data.edges) ? data.edges : []
  if (nodeValues.length === 0 && edgeValues.length === 0) {
    throw new TypeError('graph-memory update requires at least one node or edge')
  }

  const sessionId = stringAttribute(request, 'sessionId')
    ?? `${request.meta.source.type}:${request.meta.source.id}`
  const nodes = nodeValues.map(parseNode)
  const edges = edgeValues.map(parseEdge)
  const nodeResults = nodes.map(node => native.upsertNode(node, sessionId))
  const edgeResults = edges.map(edge => native.upsertEdge(edge, sessionId))
  context.signal?.throwIfAborted()
  return { nodes: nodeResults, edges: edgeResults }
}

async function retrieveGraph(
  native: GraphMemoryNative,
  request: MemoryRetrieveRequest,
  context: MemoryPluginContext,
): Promise<MemoryData> {
  context.signal?.throwIfAborted()
  const result = await native.recall(requireQuery(request.data, 'graph-memory'))
  context.signal?.throwIfAborted()
  return result
}

function parseNode(value: MemoryData): GraphNodeInput {
  const node = recordOf(value)
  if (node === undefined) throw new TypeError('graph-memory nodes must be objects')
  const type = stringValue(node.type)
  const name = stringValue(node.name)
  const description = stringValue(node.description)
  const content = stringValue(node.content)
  if (type === undefined || !nodeTypes.has(type as GraphNodeType)
    || name === undefined || description === undefined || content === undefined) {
    throw new TypeError('graph-memory nodes require type, name, description, and content')
  }
  return { type: type as GraphNodeType, name, description, content }
}

function parseEdge(value: MemoryData): GraphEdgeInput {
  const edge = recordOf(value)
  if (edge === undefined) throw new TypeError('graph-memory edges must be objects')
  const from = stringValue(edge.from)
  const to = stringValue(edge.to)
  const type = stringValue(edge.type)
  if (from === undefined || to === undefined || type === undefined) {
    throw new TypeError('graph-memory edges require from, to, and type')
  }
  const instruction = stringValue(edge.instruction)
  const condition = stringValue(edge.condition)
  return {
    from,
    to,
    type,
    ...(instruction === undefined ? {} : { instruction }),
    ...(condition === undefined ? {} : { condition }),
  }
}

function isNode(value: Record<string, MemoryData>): boolean {
  return value.type !== undefined || value.name !== undefined || value.content !== undefined
}

function stringAttribute(
  request: MemoryUpdateRequest | MemoryRetrieveRequest,
  key: string,
): string | undefined {
  return stringValue(request.meta.attributes?.[key])
}
