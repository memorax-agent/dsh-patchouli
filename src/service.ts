import { spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { createConnection, type Socket } from 'node:net'

import { Service, type Context } from '@deepseek-ai/cordis'
import {
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
  type JsonRpcSuccess,
  type ReadEntityParams,
  type ReadEntityResult,
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

export class PatchouliService extends Service {
  private socket?: Socket
  private buffer = ''
  private nextId = 1
  private readonly pending = new Map<JsonRpcId, PendingCall>()
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

  async close(): Promise<void> {
    this.closing = true
    this.handshake = undefined
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
        capabilities: [],
      })
    } catch (error) {
      socket.destroy()
      this.socket = undefined
      throw error
    }
  }

  private call<TResult>(method: string, params: unknown): Promise<TResult> {
    const socket = this.socket
    if (!socket || socket.destroyed) {
      return Promise.reject(unavailable('Patchouli daemon is not connected'))
    }
    const id = this.nextId++
    const request = JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n'
    return new Promise<TResult>((resolve, reject) => {
      this.pending.set(id, {
        resolve: value => resolve(value as TResult),
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
      let response: JsonRpcSuccess | JsonRpcFailure
      try {
        response = JSON.parse(line) as JsonRpcSuccess | JsonRpcFailure
      } catch (error) {
        this.connectionFailed(new Error('Patchouli daemon returned invalid JSON', { cause: error }))
        return
      }
      const responseId = response.id
      if (responseId === null) continue
      const pending = this.pending.get(responseId)
      if (!pending) continue
      this.pending.delete(responseId)
      if ('error' in response) {
        pending.reject(new Error(`Patchouli RPC ${response.error.code}: ${response.error.message}`))
      } else {
        pending.resolve(response.result)
      }
    }
  }

  private connectionFailed(error: Error): void {
    this.handshake = undefined
    this.socket = undefined
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
