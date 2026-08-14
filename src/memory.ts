import { Service, type Context } from '@deepseek-ai/cordis'

export type MemoryMetadata = Readonly<Record<string, unknown>>

export interface MemoryMessage {
  readonly role: 'user' | 'assistant'
  readonly content: string
}

export interface MemoryUpdateRequest {
  readonly scope: string
  readonly messages: readonly MemoryMessage[]
  readonly metadata?: MemoryMetadata
}

export interface MemoryUpdateReceipt {
  readonly status: 'accepted' | 'applied'
  readonly receipt?: string
}

export interface MemoryRetrieveRequest {
  readonly scope: string
  readonly query: string
  readonly limit?: number
  readonly metadata?: MemoryMetadata
}

export interface MemoryHit {
  readonly id?: string
  readonly content: string
  readonly score?: number
  readonly metadata?: MemoryMetadata
}

export interface MemoryRetrieveResult {
  readonly items: readonly MemoryHit[]
}

export interface MemoryPluginContext {
  readonly signal?: AbortSignal
}

/** A concrete memory implementation registered with the common frontend. */
export interface MemoryPlugin {
  readonly id: string
  update(
    request: MemoryUpdateRequest,
    context: MemoryPluginContext,
  ): Promise<MemoryUpdateReceipt>
  retrieve(
    request: MemoryRetrieveRequest,
    context: MemoryPluginContext,
  ): Promise<MemoryRetrieveResult>
}

export type MemoryPluginOutcome<T> =
  | {
      readonly pluginId: string
      readonly ok: true
      readonly value: T
    }
  | {
      readonly pluginId: string
      readonly ok: false
      readonly error: string
    }

declare module '@deepseek-ai/cordis' {
  interface Context {
    patchouliMemory: MemoryService
  }
}

/** Cordis service that registers, routes to, and aggregates memory plugins. */
export class MemoryService extends Service {
  private readonly plugins = new Map<string, MemoryPlugin>()

  constructor(ctx: Context) {
    super(ctx, 'patchouliMemory')
  }

  register(plugin: MemoryPlugin): () => void {
    if (plugin.id.trim() === '') {
      throw new Error('memory plugin id must be a non-empty string')
    }
    if (this.plugins.has(plugin.id)) {
      throw new Error(`memory plugin "${plugin.id}" is already registered`)
    }

    const dispose = this.ctx.effect(() => {
      this.plugins.set(plugin.id, plugin)
      return () => {
        if (this.plugins.get(plugin.id) === plugin) {
          this.plugins.delete(plugin.id)
        }
      }
    }, `patchouliMemory.register(${JSON.stringify(plugin.id)})`)

    return () => void dispose()
  }

  update(
    request: MemoryUpdateRequest,
    signal?: AbortSignal,
  ): Promise<readonly MemoryPluginOutcome<MemoryUpdateReceipt>[]> {
    return this.dispatch(plugin => plugin.update(request, { signal }), signal)
  }

  retrieve(
    request: MemoryRetrieveRequest,
    signal?: AbortSignal,
  ): Promise<readonly MemoryPluginOutcome<MemoryRetrieveResult>[]> {
    return this.dispatch(plugin => plugin.retrieve(request, { signal }), signal)
  }

  private async dispatch<T>(
    invoke: (plugin: MemoryPlugin) => Promise<T>,
    signal?: AbortSignal,
  ): Promise<readonly MemoryPluginOutcome<T>[]> {
    signal?.throwIfAborted()
    const plugins = [...this.plugins.values()]
    const outcomes = await Promise.all(plugins.map(async (plugin): Promise<MemoryPluginOutcome<T>> => {
      try {
        return {
          pluginId: plugin.id,
          ok: true,
          value: await invoke(plugin),
        }
      } catch (error) {
        return {
          pluginId: plugin.id,
          ok: false,
          error: error instanceof Error ? error.message : String(error),
        }
      }
    }))
    signal?.throwIfAborted()
    return outcomes
  }
}
