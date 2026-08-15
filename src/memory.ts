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

export interface MemorySubscribeRequest {
  readonly scope: string
  readonly metadata?: MemoryMetadata
}

/** A provider-local change. Cursors are opaque and may only be compared for equality. */
export interface MemoryChange {
  readonly cursor: string
  readonly memoryId?: string
  readonly metadata?: MemoryMetadata
}

export interface MemoryChangeEvent extends MemoryChange {
  readonly pluginId: string
}

export interface MemoryPluginSubscribeRequest extends MemorySubscribeRequest {
  readonly afterCursor?: string
}

export type MemoryPluginChangeHandler = (change: MemoryChange) => void | Promise<void>
export type MemoryChangeHandler = (change: MemoryChangeEvent) => void | Promise<void>

export interface MemoryPluginSubscription {
  /** Boundary captured by the provider before live changes are delivered. */
  readonly cursor: string
  /** Reject with a retryable MemorySubscriptionError for an unexpected disconnect. */
  readonly closed: Promise<void>
  unsubscribe(): Promise<void>
}

/** Cursor storage already bound to one consumer, subscription, and scope. */
export interface MemoryCursorStore {
  load(pluginId: string): Promise<string | undefined>
  save(pluginId: string, cursor: string): Promise<void>
  /** Clear a cursor only after the consumer has completed its explicit resync. */
  delete(pluginId: string): Promise<void>
}

export interface MemorySubscriptionErrorOptions {
  readonly retryable?: boolean
  readonly resetRequired?: boolean
  readonly cause?: unknown
}

/** A classified subscription failure. Unknown failures are always fatal. */
export class MemorySubscriptionError extends Error {
  readonly retryable: boolean
  readonly resetRequired: boolean

  constructor(message: string, options: MemorySubscriptionErrorOptions = {}) {
    super(message, { cause: options.cause })
    this.name = 'MemorySubscriptionError'
    this.retryable = options.retryable ?? false
    this.resetRequired = options.resetRequired ?? false
  }
}

export interface MemorySubscriptionFailure {
  readonly pluginId: string
  readonly error: MemorySubscriptionError
}

export interface MemorySubscriptionOptions {
  readonly cursorStore: MemoryCursorStore
  readonly onError?: (failure: MemorySubscriptionFailure) => void
  readonly signal?: AbortSignal
}

export interface MemorySubscription {
  readonly pluginIds: readonly string[]
  readonly closed: Promise<void>
  unsubscribe(): Promise<void>
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
  subscribe?(
    request: MemoryPluginSubscribeRequest,
    handler: MemoryPluginChangeHandler,
    context: MemoryPluginContext,
  ): Promise<MemoryPluginSubscription>
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

  async subscribe(
    request: MemorySubscribeRequest,
    handler: MemoryChangeHandler,
    options: MemorySubscriptionOptions,
  ): Promise<MemorySubscription> {
    options.signal?.throwIfAborted()
    const plugins = [...this.plugins.values()].filter(
      (plugin): plugin is MemoryPlugin & Required<Pick<MemoryPlugin, 'subscribe'>> =>
        plugin.subscribe !== undefined,
    )
    const pluginIds = Object.freeze(plugins.map(plugin => plugin.id))
    const lifetime = new AbortController()
    const active = new Map<string, ManagedPluginSubscription>()
    const workers: Promise<void>[] = []
    const aborted = new Promise<never>((_resolve, reject) => {
      lifetime.signal.addEventListener('abort', () => {
        reject(lifetime.signal.reason ?? new Error('memory subscription aborted'))
      }, { once: true })
    })
    void aborted.catch(() => {})

    let resolveClosed!: () => void
    const closed = new Promise<void>((resolve) => {
      resolveClosed = resolve
    })
    let shutdownTask: Promise<void> | undefined
    let onAbort: (() => void) | undefined
    const shutdown = (): Promise<void> => shutdownTask ??= (async () => {
      lifetime.abort(options.signal?.reason ?? new Error('memory subscription disposed'))
      if (onAbort) options.signal?.removeEventListener('abort', onAbort)
      await Promise.allSettled([...active.values()].map(subscription => subscription.stop()))
      await Promise.allSettled(workers)
      resolveClosed()
    })()

    const dispose = this.ctx.effect(
      () => shutdown,
      `patchouliMemory.subscribe(${JSON.stringify(pluginIds)})`,
    )
    onAbort = () => void dispose()
    options.signal?.addEventListener('abort', onAbort, { once: true })

    if (options.signal?.aborted) {
      void dispose()
    } else {
      for (const plugin of plugins) {
        workers.push(this.runSubscriptionWorker(
          plugin,
          request,
          handler,
          options,
          lifetime.signal,
          aborted,
          active,
        ))
      }
    }

    return {
      pluginIds,
      closed,
      async unsubscribe() {
        await dispose()
      },
    }
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

  private async runSubscriptionWorker(
    plugin: MemoryPlugin & Required<Pick<MemoryPlugin, 'subscribe'>>,
    request: MemorySubscribeRequest,
    handler: MemoryChangeHandler,
    options: MemorySubscriptionOptions,
    signal: AbortSignal,
    aborted: Promise<never>,
    active: Map<string, ManagedPluginSubscription>,
  ): Promise<void> {
    const cursor = { value: undefined as string | undefined }
    let loaded = false
    let retry = 0

    while (!signal.aborted) {
      try {
        if (!loaded) {
          cursor.value = await options.cursorStore.load(plugin.id)
          loaded = true
        }
        await this.runSubscriptionAttempt(
          plugin,
          request,
          handler,
          options.cursorStore,
          cursor,
          () => {
            retry = 0
          },
          signal,
          aborted,
          active,
        )
        return
      } catch (error: unknown) {
        if (signal.aborted) return
        const classified = classifySubscriptionError(error)
        notifySubscriptionError(options.onError, {
          pluginId: plugin.id,
          error: classified,
        })
        // Reset requires an explicit consumer resync; never discard its cursor here.
        if (classified.resetRequired || !classified.retryable) return
        if (!await retryDelay(retry++, signal)) return
      }
    }
  }

  private async runSubscriptionAttempt(
    plugin: MemoryPlugin & Required<Pick<MemoryPlugin, 'subscribe'>>,
    request: MemorySubscribeRequest,
    handler: MemoryChangeHandler,
    cursorStore: MemoryCursorStore,
    cursor: { value: string | undefined },
    onProgress: () => void,
    signal: AbortSignal,
    aborted: Promise<never>,
    active: Map<string, ManagedPluginSubscription>,
  ): Promise<void> {
    let releaseBoundary!: () => void
    const boundary = new Promise<void>((resolve) => {
      releaseBoundary = resolve
    })
    let boundaryError: unknown | typeof noSubscriptionError = noSubscriptionError
    let pipelineError: unknown | typeof noSubscriptionError = noSubscriptionError
    let accepting = true
    let failAttempt!: (error: unknown) => void
    const failed = new Promise<never>((_resolve, reject) => {
      failAttempt = reject
    })
    void failed.catch(() => {})
    let tail = Promise.resolve()

    const onChange: MemoryPluginChangeHandler = (change) => {
      if (!accepting || signal.aborted) return Promise.resolve()
      const processing = tail.then(async () => {
        await boundary
        if (boundaryError !== noSubscriptionError) throw boundaryError
        if (pipelineError !== noSubscriptionError) throw pipelineError
        if (change.cursor === cursor.value) return
        try {
          signal.throwIfAborted()
          await handler({ ...change, pluginId: plugin.id })
          signal.throwIfAborted()
          await cursorStore.save(plugin.id, change.cursor)
          cursor.value = change.cursor
          onProgress()
        } catch (error: unknown) {
          pipelineError = error
          throw error
        }
      })
      tail = processing.then(
        () => undefined,
        (error: unknown) => failAttempt(error),
      )
      return processing
    }

    let managed: ManagedPluginSubscription | undefined
    let attemptError: unknown | typeof noSubscriptionError = noSubscriptionError
    try {
      const starting = plugin.subscribe({
        scope: request.scope,
        metadata: request.metadata,
        afterCursor: cursor.value,
      }, onChange, { signal })
      let subscription: MemoryPluginSubscription
      try {
        subscription = await Promise.race([starting, aborted])
      } catch (error: unknown) {
        if (signal.aborted) {
          void starting.then(
            late => managePluginSubscription(late).stop().catch(() => {}),
            () => {},
          )
        }
        throw error
      }

      managed = managePluginSubscription(subscription)
      active.set(plugin.id, managed)
      signal.throwIfAborted()
      await cursorStore.save(plugin.id, subscription.cursor)
      cursor.value = subscription.cursor
      releaseBoundary()
      await Promise.race([managed.closed, failed, aborted])
    } catch (error: unknown) {
      attemptError = error
      boundaryError = error
    } finally {
      accepting = false
      releaseBoundary()
      if (managed) {
        try {
          await managed.stop()
        } catch (error: unknown) {
          if (attemptError === noSubscriptionError) attemptError = error
        }
        if (active.get(plugin.id) === managed) active.delete(plugin.id)
      }
      await tail
      if (attemptError === noSubscriptionError) attemptError = pipelineError
    }

    if (attemptError !== noSubscriptionError) throw attemptError
  }
}

const noSubscriptionError = Symbol('no subscription error')

interface ManagedPluginSubscription {
  readonly closed: Promise<void>
  stop(): Promise<void>
}

function managePluginSubscription(subscription: MemoryPluginSubscription): ManagedPluginSubscription {
  const closed = subscription.closed
  // Observe disconnects immediately, before boundary persistence or teardown can await.
  void closed.catch(() => {})
  let stopping: Promise<void> | undefined
  return {
    closed,
    stop() {
      return stopping ??= subscription.unsubscribe()
    },
  }
}

function classifySubscriptionError(error: unknown): MemorySubscriptionError {
  if (error instanceof MemorySubscriptionError) return error
  return new MemorySubscriptionError(
    error instanceof Error ? error.message : String(error),
    { cause: error },
  )
}

function notifySubscriptionError(
  onError: MemorySubscriptionOptions['onError'],
  failure: MemorySubscriptionFailure,
): void {
  try {
    onError?.(failure)
  } catch {
    // Consumer diagnostics must not stop another plugin's worker.
  }
}

function retryDelay(retry: number, signal: AbortSignal): Promise<boolean> {
  const base = Math.min(30_000, 250 * 2 ** Math.min(retry, 16))
  const delay = base + Math.floor(Math.random() * Math.min(base, 1_000))
  return new Promise((resolve) => {
    if (signal.aborted) return resolve(false)
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', abort)
      resolve(true)
    }, delay)
    const abort = () => {
      clearTimeout(timer)
      resolve(false)
    }
    signal.addEventListener('abort', abort, { once: true })
  })
}
