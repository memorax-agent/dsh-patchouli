import type { Context } from '@deepseek-ai/cordis'
import type {
  CreateEntityParams,
  DeleteEntityParams,
  ReadEntityParams,
  RetrieveEntitiesParams,
  UpdateEntityParams,
} from 'dsh-patchouli-protocol'
import type { MemoryCallMeta, MemoryData, MemoryPlugin } from 'dsh-patchouli'
import type {} from 'dsh-patchouli/storage'

export const name = 'dsh-patchouli-crud-test-plugin'

export const inject = ['patchouli', 'patchouliStorage'] as const

export type CrudTestOperation = 'create' | 'read' | 'retrieve' | 'update' | 'delete'

const pluginId = 'crud-test'
const sourceType = 'crud-test'

function operation(meta: MemoryCallMeta): CrudTestOperation {
  const value = meta.attributes?.operation
  if (
    value === 'create'
    || value === 'read'
    || value === 'retrieve'
    || value === 'update'
    || value === 'delete'
  ) return value
  throw new Error('crud-test calls require a supported meta.attributes.operation')
}

function params<T>(data: MemoryData): T {
  return data as T
}

function result(data: unknown): MemoryData {
  return data as MemoryData
}

export function apply(ctx: Context): () => void {
  const plugin: MemoryPlugin = {
    id: pluginId,
    async update(request) {
      switch (operation(request.meta)) {
        case 'create':
          return result(await ctx.patchouliStorage.create(params<CreateEntityParams>(request.data)))
        case 'update':
          return result(await ctx.patchouliStorage.update(params<UpdateEntityParams>(request.data)))
        case 'delete':
          return result(await ctx.patchouliStorage.delete(params<DeleteEntityParams>(request.data)))
        case 'read':
        case 'retrieve':
          throw new Error('crud-test read operations must use patchouli.retrieve')
      }
    },
    async retrieve(request) {
      switch (operation(request.meta)) {
        case 'read':
          return result(await ctx.patchouliStorage.read(params<ReadEntityParams>(request.data)))
        case 'retrieve':
          return result(await ctx.patchouliStorage.retrieve(params<RetrieveEntitiesParams>(request.data)))
        case 'create':
        case 'update':
        case 'delete':
          throw new Error('crud-test mutations must use patchouli.update')
      }
    },
  }

  return ctx.patchouli.register(plugin, {
    filter: call => call.meta.source.type === sourceType,
  })
}
