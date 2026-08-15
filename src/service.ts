import { spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { createConnection, type Socket } from 'node:net'

import { Service, type Context } from '@deepseek-ai/cordis'
import {
  type ChangesEventParams,
  type CreateEntityParams,
  type ControlCheckpointResult,
  type DeleteEntityParams,
  type JsonValue,
  type MutationResult,
  methods,
  protocolVersion,
  type ControlStatusResult,
  type HandshakeResult,
  type JsonRpcFailure,
  type JsonRpcId,
  type JsonRpcNotification,
  type JsonRpcSuccess,
  type ReadEntityParams,
  type ReadEntityResult,
  type RetrieveEntitiesParams,
  type RetrieveEntitiesResult,
  type SubscribeChangesParams,
  type SubscribeChangesResult,
  type UnsubscribeChangesParams,
  type UnsubscribeChangesResult,
  type UpdateEntityParams,
} from '@memorax-agent/patchouli-protocol'

import type { Config } from './storage.js'

declare module '@deepseek-ai/cordis' {
  interface Context {
    patchouli: PatchouliService
  }
}

interface PendingCall {
  resolve(value: JsonValue): void
  reject(error: Error): void
}

/** Called in wire order; handlers own any required async serialization. */
export type ChangeHandler<TType extends string = string> = (
  event: ChangesEventParams<TType>,
) => void | Promise<void>

const subscriptionsCapability = 'subscriptions'

export class PatchouliService extends Service {
  private socket?: Socket
  private buffer = ''
  private nextId = 1
  private readonly pending = new Map<JsonRpcId, PendingCall>()
  private readonly changeHandlers = new Map<string, ChangeHandler>()
  private handshake?: HandshakeResult
  private closing = false

  constructor(ctx: Context, private readonly config: Config) {
    super(ctx, 'patchouli')
  }

  get server(): HandshakeResult | undefined {
    return this.handshake
  }

  async start(): Promise<void> {
    try {
      await this.connect()
    } catch (error) {
      if (!this.config.autoStart || !isUnavailable(error)) throw error
      await startDaemon(
        this.config.command,
        this.config.endpoint,
        this.config.providerConfigPath,
        this.config.backendConfigPath,
      )
      await this.waitForDaemon()
    }
    this.ctx.logger('patchouli').info(
      'connected to daemon node %s at %s',
      this.handshake?.server.node_id,
      this.config.endpoint,
    )
  }

  async status(): Promise<ControlStatusResult> {
    return this.call<ControlStatusResult>(methods.controlStatus, { meta: {}, data: {} })
  }

  async checkpoint(): Promise<ControlCheckpointResult> {
    return this.call<ControlCheckpointResult>(methods.controlCheckpoint, { meta: {}, data: {} })
  }

  async create<TType extends string = string, TValue extends JsonValue = JsonValue>(
    params: CreateEntityParams<TType, TValue>,
  ): Promise<MutationResult<TType, TValue>> {
    return this.call<MutationResult<TType, TValue>>(methods.entityCreate, params)
  }

  async read<TType extends string = string, TValue extends JsonValue = JsonValue>(
    params: ReadEntityParams<TType>,
  ): Promise<ReadEntityResult<TType, TValue>> {
    return this.call<ReadEntityResult<TType, TValue>>(methods.entityRead, params)
  }

  async retrieve<TType extends string = string, TValue extends JsonValue = JsonValue>(
    params: RetrieveEntitiesParams<TType>,
  ): Promise<RetrieveEntitiesResult<TType, TValue>> {
    return this.call<RetrieveEntitiesResult<TType, TValue>>(methods.entityRetrieve, params)
  }

  async update<TType extends string = string, TValue extends JsonValue = JsonValue>(
    params: UpdateEntityParams<TType, TValue>,
  ): Promise<MutationResult<TType, TValue>> {
    return this.call<MutationResult<TType, TValue>>(methods.entityUpdate, params)
  }

  async delete<TType extends string = string>(
    params: DeleteEntityParams<TType>,
  ): Promise<MutationResult<TType, JsonValue>> {
    return this.call<MutationResult<TType, JsonValue>>(methods.entityDelete, params)
  }

  async subscribe<TType extends string = string>(
    params: SubscribeChangesParams<TType>,
    handler: ChangeHandler<TType>,
  ): Promise<SubscribeChangesResult> {
    if (this.handshake && !this.handshake.capabilities.includes(subscriptionsCapability)) {
      throw new Error('Patchouli daemon did not negotiate change subscriptions')
    }
    return this.call<SubscribeChangesResult>(methods.changesSubscribe, params, (result) => {
      this.changeHandlers.set(
        result.data.subscription_id,
        event => handler(event as ChangesEventParams<TType>),
      )
    })
  }

  async unsubscribe(params: UnsubscribeChangesParams): Promise<UnsubscribeChangesResult> {
    return this.call<UnsubscribeChangesResult>(methods.changesUnsubscribe, params, () => {
      this.changeHandlers.delete(params.data.subscription_id)
    })
  }

  async close(): Promise<void> {
    this.closing = true
    this.handshake = undefined
    this.changeHandlers.clear()
    this.socket?.destroy()
    this.socket = undefined
    this.rejectPending(new Error('Patchouli connection closed'))
  }

  private async waitForDaemon(): Promise<void> {
    const deadline = Date.now() + this.config.startupTimeoutMs
    let lastError: unknown
    while (Date.now() < deadline) {
      try {
        await this.connect()
        return
      } catch (error) {
        if (!isUnavailable(error)) throw error
        lastError = error
        await delay(50)
      }
    }
    throw new Error(
      `Patchouli daemon did not become ready within ${this.config.startupTimeoutMs}ms`,
      { cause: lastError },
    )
  }

  private async connect(): Promise<void> {
    this.closing = false
    const socket = createConnection(this.config.endpoint)
    try {
      await new Promise<void>((resolve, reject) => {
        socket.once('connect', resolve)
        socket.once('error', reject)
      })
    } catch (error) {
      socket.destroy()
      throw error
    }

    socket.setEncoding('utf8')
    socket.on('data', chunk => this.receive(chunk.toString()))
    socket.on('error', error => this.connectionFailed(error))
    socket.on('close', () => this.connectionFailed(new Error('Patchouli daemon disconnected')))
    this.socket = socket

    try {
      this.handshake = await this.call<HandshakeResult>(methods.handshake, {
        client: {
          name: 'dsh-patchouli',
          version: '0.1.0',
          instance_id: randomUUID(),
        },
        protocol_versions: [protocolVersion],
        capabilities: [subscriptionsCapability],
      })
    } catch (error) {
      socket.destroy()
      this.socket = undefined
      throw error
    }
  }

  private call<TResult>(
    method: string,
    params: unknown,
    onResult?: (result: TResult) => void,
  ): Promise<TResult> {
    const socket = this.socket
    if (!socket || socket.destroyed) {
      return Promise.reject(unavailable('Patchouli daemon is not connected'))
    }
    const id = this.nextId++
    const request = JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n'
    return new Promise<TResult>((resolve, reject) => {
      this.pending.set(id, {
        resolve: (value) => {
          try {
            const result = value as TResult
            onResult?.(result)
            resolve(result)
          } catch (error) {
            reject(error)
          }
        },
        reject,
      })
      socket.write(request, error => {
        if (!error) return
        this.pending.delete(id)
        reject(error)
      })
    })
  }

  private receive(chunk: string): void {
    this.buffer += chunk
    while (true) {
      const newline = this.buffer.indexOf('\n')
      if (newline < 0) return
      const line = this.buffer.slice(0, newline).trimEnd()
      this.buffer = this.buffer.slice(newline + 1)
      if (!line) continue
      let message: JsonRpcSuccess | JsonRpcFailure | JsonRpcNotification<ChangesEventParams>
      try {
        message = JSON.parse(line) as
          | JsonRpcSuccess
          | JsonRpcFailure
          | JsonRpcNotification<ChangesEventParams>
      } catch (error) {
        this.connectionFailed(new Error('Patchouli daemon returned invalid JSON', { cause: error }))
        return
      }
      if ('method' in message) {
        this.receiveNotification(message)
        continue
      }
      const responseId = message.id
      if (responseId === null) continue
      const pending = this.pending.get(responseId)
      if (!pending) continue
      this.pending.delete(responseId)
      if ('error' in message) {
        pending.reject(new Error(`Patchouli RPC ${message.error.code}: ${message.error.message}`))
      } else {
        pending.resolve(message.result)
      }
    }
  }

  private receiveNotification(notification: JsonRpcNotification<ChangesEventParams>): void {
    if (notification.method !== methods.changesEvent) return
    const subscriptionId = notification.params.data.subscription_id
    const handler = this.changeHandlers.get(subscriptionId)
    if (!handler) return
    try {
      const handling = handler(notification.params)
      void handling?.catch(error => this.warnChangeHandler(subscriptionId, error))
    } catch (error) {
      this.warnChangeHandler(subscriptionId, error)
    }
  }

  private warnChangeHandler(subscriptionId: string, error: unknown): void {
    const message = error instanceof Error ? error.message : String(error)
    this.ctx.logger('patchouli').warn(
      'change handler for subscription %s failed: %s',
      subscriptionId,
      message,
    )
  }

  private connectionFailed(error: Error): void {
    this.handshake = undefined
    this.socket = undefined
    this.changeHandlers.clear()
    this.rejectPending(error)
    if (!this.closing) this.ctx.logger('patchouli').warn(error)
  }

  private rejectPending(error: Error): void {
    for (const pending of this.pending.values()) pending.reject(error)
    this.pending.clear()
  }
}

async function startDaemon(
  command: string,
  endpoint: string,
  providerConfigPath: string,
  backendConfigPath: string,
): Promise<void> {
  const child = spawn(command, [
    'serve',
    '--endpoint', endpoint,
    '--providers', providerConfigPath,
    '--config', backendConfigPath,
  ], {
    detached: true,
    stdio: 'ignore',
  })
  await new Promise<void>((resolve, reject) => {
    child.once('spawn', resolve)
    child.once('error', reject)
  })
  child.unref()
}

function isUnavailable(error: unknown): boolean {
  return error instanceof Error
    && ('code' in error
      ? ['ECONNREFUSED', 'ENOENT', 'EPIPE'].includes(String(error.code))
      : error.message === 'Patchouli daemon is not connected')
}

function unavailable(message: string): Error & { code: string } {
  return Object.assign(new Error(message), { code: 'ENOENT' })
}

function delay(milliseconds: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, milliseconds))
}
