import { realpathSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'

import type { Context } from '@deepseek-ai/cordis'
import type { HarmonyService } from 'dsh-harmony'

import type { MemoryData } from '../memory.js'
import type { GraphMemoryNative } from './adapters/graph-memory.js'

export const name = 'dsh-patchouli-goojfc-graph-memory'
export const inject = ['patchouliGoojfc', 'harmony'] as const

interface GraphConfig extends Record<string, unknown> {
  readonly dbPath: string
  readonly embedding?: unknown
}

interface GraphDatabase {}

interface GraphDbModule {
  getDb(path: string): GraphDatabase
  closeDb(): void
}

interface GraphStoreModule {
  upsertNode(
    db: GraphDatabase,
    node: Parameters<GraphMemoryNative['upsertNode']>[0],
    sessionId: string,
  ): { node: MemoryData }
  findByName(db: GraphDatabase, name: string): { id: string } | undefined
  upsertEdge(
    db: GraphDatabase,
    edge: Record<string, unknown>,
  ): MemoryData
}

interface GraphTypesModule {
  readonly DEFAULT_CONFIG: GraphConfig
}

interface GraphRecaller {
  recall(query: string): Promise<MemoryData>
  syncEmbed(node: MemoryData): Promise<unknown>
  setEmbedFn(fn: (text: string) => Promise<number[]>): void
}

interface GraphRecallerModule {
  readonly Recaller: new (db: GraphDatabase, config: GraphConfig) => GraphRecaller
}

interface GraphEmbedModule {
  createEmbedFn(config: unknown): Promise<((text: string) => Promise<number[]>) | null>
}

export async function apply(
  ctx: Context & { harmony: HarmonyService },
  rawConfig: Record<string, unknown> = {},
): Promise<() => void> {
  const profileRequire = createRequire(join(ctx.harmony.profile().dir, 'package.json'))
  const root = realpathSync(dirname(profileRequire.resolve('graph-memory/package.json')))
  const [dbModule, store, types, recallerModule, embed] = await Promise.all([
    importGraphModule<GraphDbModule>(root, 'src/store/db.ts'),
    importGraphModule<GraphStoreModule>(root, 'src/store/store.ts'),
    importGraphModule<GraphTypesModule>(root, 'src/types.ts'),
    importGraphModule<GraphRecallerModule>(root, 'src/recaller/recall.ts'),
    importGraphModule<GraphEmbedModule>(root, 'src/engine/embed.ts'),
  ])
  const config = graphConfig(types.DEFAULT_CONFIG, rawConfig)
  const db = dbModule.getDb(config.dbPath)
  const recaller = new recallerModule.Recaller(db, config)
  let active = true
  void embed.createEmbedFn(config.embedding).then((fn) => {
    if (active && fn !== null) recaller.setEmbedFn(fn)
  }).catch(() => undefined)

  const adapter = ctx.patchouliGoojfc.createGraphMemoryAdapter({
    recall: query => recaller.recall(query),
    upsertNode(node, sessionId) {
      const result = store.upsertNode(db, node, sessionId)
      void recaller.syncEmbed(result.node).catch(() => undefined)
      return result
    },
    upsertEdge(edge, sessionId) {
      const fromId = store.findByName(db, edge.from)?.id
      const toId = store.findByName(db, edge.to)?.id
      if (fromId === undefined || toId === undefined) {
        throw new Error('graph-memory edge endpoints must exist')
      }
      return store.upsertEdge(db, {
        fromId,
        toId,
        type: edge.type,
        instruction: edge.instruction,
        condition: edge.condition,
        sessionId,
      })
    },
  })
  const unprovide = ctx.provide('goojfcGraphMemory', adapter)

  return () => {
    active = false
    unprovide()
    dbModule.closeDb()
  }
}

function graphConfig(
  defaults: GraphConfig,
  rawConfig: Record<string, unknown>,
): GraphConfig {
  const config = { ...defaults, ...rawConfig }
  if (typeof config.dbPath !== 'string' || config.dbPath.trim() === '') {
    throw new TypeError('graph-memory dbPath must be a non-empty string')
  }
  return config as GraphConfig
}

function importGraphModule<T>(root: string, path: string): Promise<T> {
  return import(pathToFileURL(join(root, path)).href) as Promise<T>
}
