import assert from 'node:assert/strict'
import { randomUUID } from 'node:crypto'
import { mkdtemp, rm } from 'node:fs/promises'
import { createServer, type Socket } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'
import type { ChangesEventParams } from 'dsh-patchouli-protocol'

import * as storage from '../lib/storage.js'

test('bridges storage requests and change notifications', async (t) => {
  const directory = await mkdtemp(join(tmpdir(), 'patchouli-storage-'))
  const endpoint = process.platform === 'win32'
    ? String.raw`\\.\pipe\patchouli-storage-${process.pid}-${randomUUID()}`
    : join(directory, 'patchouli.sock')
  let handshakeCapabilities: unknown
  let serverSocket: Socket | undefined
  let nextSubscriptionId = 1
  let unsubscribeRequests = 0
  const retrievalRequests: Array<{ data: { query: string } }> = []
  const successfulCreateRequests: Array<{ meta: Record<string, unknown> }> = []
  const server = createServer((socket) => {
    serverSocket = socket
    let buffer = ''
    socket.setEncoding('utf8')
    socket.on('data', (chunk) => {
      buffer += chunk
      while (true) {
        const newline = buffer.indexOf('\n')
        if (newline < 0) return
        const request = JSON.parse(buffer.slice(0, newline))
        buffer = buffer.slice(newline + 1)
        if (request.method === 'patchouli.control.checkpoint@1') {
          socket.write(JSON.stringify({
            jsonrpc: '2.0',
            id: request.id,
            result: { meta: {}, data: { completed: true } },
          }) + '\n')
          continue
        }
        if (request.method === 'patchouli.entity.retrieve@1') {
          retrievalRequests.push(request.params)
          const instruction = JSON.parse(request.params.data.query)
          const secondPage = instruction.cursor === 'cursor-page-1'
          const paged = instruction.order === 'id_asc'
          socket.write(JSON.stringify({
            jsonrpc: '2.0',
            id: request.id,
            result: {
              meta: paged && !secondPage ? { next_cursor: 'cursor-page-1' } : {},
              data: {
                hits: [{
                  score: 1,
                  variants: [{
                    ref: { type: 'event', id: secondPage ? 'event-2' : 'event-1' },
                    version: 'version-1',
                    state: 'active',
                    value: { payload: 'hello' },
                  }],
                }],
              },
            },
          }) + '\n')
          continue
        }
        if (request.method === 'patchouli.entity.create@1') {
          if (request.params.meta.fixture_success === true) {
            successfulCreateRequests.push(request.params)
            socket.write(JSON.stringify({
              jsonrpc: '2.0',
              id: request.id,
              result: {
                meta: {},
                data: {
                  entity: {
                    ref: {
                      type: request.params.data.type,
                      id: request.params.data.id,
                    },
                    version: `version-${successfulCreateRequests.length}`,
                    state: 'active',
                    value: request.params.data.value,
                  },
                },
              },
            }) + '\n')
            continue
          }
          socket.write(JSON.stringify({
            jsonrpc: '2.0',
            id: request.id,
            error: {
              code: -32006,
              message: 'entity create is unavailable in this fixture',
              data: { reason: 'UNSUPPORTED_CAPABILITY' },
            },
          }) + '\n')
          continue
        }
        if (request.method === 'patchouli.changes.subscribe@1') {
          const subscriptionId = `subscription-${nextSubscriptionId++}`
          socket.write([
            JSON.stringify({
              jsonrpc: '2.0',
              id: request.id,
              result: {
                meta: {},
                data: { subscription_id: subscriptionId, cursor: 'cursor-0' },
              },
            }),
            JSON.stringify({
              jsonrpc: '2.0',
              method: 'patchouli.changes.event@1',
              params: {
                meta: { transaction_id: 'transaction-1' },
                data: {
                  subscription_id: subscriptionId,
                  change: {
                    cursor: 'cursor-1',
                    ref: { type: 'event', id: 'event-1' },
                    kind: 'created',
                    head_versions: ['version-1'],
                  },
                },
              },
            }),
          ].join('\n') + '\n')
          continue
        }
        if (request.method === 'patchouli.changes.unsubscribe@1') {
          unsubscribeRequests += 1
          socket.write([
            JSON.stringify({
              jsonrpc: '2.0',
              id: request.id,
              result: { meta: {}, data: { removed: true } },
            }),
            JSON.stringify({
              jsonrpc: '2.0',
              method: 'patchouli.changes.event@1',
              params: {
                meta: {},
                data: {
                  subscription_id: request.params.data.subscription_id,
                  change: {
                    cursor: 'cursor-2',
                    ref: { type: 'event', id: 'event-1' },
                    kind: 'updated',
                    head_versions: ['version-2'],
                  },
                },
              },
            }),
          ].join('\n') + '\n')
          continue
        }
        if (request.method === 'patchouli.protocol.handshake@1') {
          handshakeCapabilities = request.params.capabilities
        }
        const result = request.method === 'patchouli.protocol.handshake@1'
          ? {
              protocol_version: 1,
              server: { version: '0.1.0', cluster_id: 'test', node_id: 'test' },
              capabilities: ['artifacts', 'subscriptions'],
              limits: {
                max_request_bytes: 1_048_576,
                max_artifact_chunk_bytes: 524_288,
                max_result_items: 1,
                idempotency_retention_seconds: 1,
                change_retention_seconds: 1,
              },
            }
          : {
              meta: {},
              data: {
                ready: true,
                provider: 'sqlite',
                generation: 4,
                recovered_after_unclean_shutdown: false,
                pid: process.pid,
                started_at_unix_ms: 1,
                active_connections: 1,
              },
            }
        socket.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\n')
      }
    })
  })

  await new Promise<void>((resolve, reject) => {
    server.once('error', reject)
    server.listen(endpoint, resolve)
  })
  const ctx = new Context()
  t.after(async () => {
    await ctx.fiber.dispose()
    await new Promise<void>((resolve, reject) => {
      server.close(error => error === undefined ? resolve() : reject(error))
    })
    await rm(directory, { recursive: true })
  })

  assert.equal(storage.name, 'dsh-patchouli-storage')
  assert.deepEqual(storage.inject, [])
  await ctx.plugin(storage, {
    endpoint,
    command: 'patchouli',
    providerConfigPath: join(directory, 'providers.json'),
    backendConfigPath: join(directory, 'config.json'),
    artifactRootPath: join(directory, 'artifacts'),
    autoStart: false,
    startupTimeoutMs: 100,
  })

  assert.equal(ctx.patchouliStorage.server?.server.node_id, 'test')
  assert.deepEqual(handshakeCapabilities, ['artifacts', 'subscriptions'])
  const status = await ctx.patchouliStorage.status()
  assert.equal(status.data.ready, true)
  assert.equal(status.data.provider, 'sqlite')
  assert.equal(status.data.generation, 4)
  assert.equal(status.data.recovered_after_unclean_shutdown, false)
  assert.equal(status.data.pid, process.pid)
  assert.equal((await ctx.patchouliStorage.checkpoint()).data.completed, true)
  const retrieval = await ctx.patchouliStorage.retrieve({
    meta: { workspace: 'test' },
    data: { query: JSON.stringify({ text: 'hello' }), types: ['event'], limit: 1 },
  })
  assert.equal(retrieval.data.hits.length, 1)
  const hit = retrieval.data.hits[0]
  assert.ok(hit)
  assert.equal(hit.score, 1)
  const variant = hit.variants[0]
  assert.ok(variant)
  assert.equal(variant.ref.id, 'event-1')

  await ctx.patchouliStorage.query(
    { workspace: 'test' },
    { text: 'hello', where: { '/payload': 'hello' } },
    { types: ['event'], limit: 1 },
  )
  assert.deepEqual(
    JSON.parse(retrievalRequests.at(-1)!.data.query),
    { text: 'hello', where: { '/payload': 'hello' } },
  )

  const pageIds: string[] = []
  for await (const page of ctx.patchouliStorage.queryPages(
    { workspace: 'test' },
    { order: 'id_asc' },
    { types: ['event'], limit: 1 },
  )) {
    pageIds.push(page.data.hits[0]!.variants[0]!.ref.id)
  }
  assert.deepEqual(pageIds, ['event-1', 'event-2'])
  assert.deepEqual(
    JSON.parse(retrievalRequests.at(-1)!.data.query),
    { order: 'id_asc', cursor: 'cursor-page-1' },
  )

  await ctx.patchouliStorage.retrieveByIds(
    { workspace: 'test' },
    ['event-1', 'event-2'],
    { types: ['event'], limit: 2 },
  )
  assert.deepEqual(
    JSON.parse(retrievalRequests.at(-1)!.data.query),
    { ids: ['event-1', 'event-2'] },
  )

  const workUnit = await ctx.patchouliStorage.runWorkUnit(
    { workspace: 'test', work_id: 'work-1', fixture_success: true },
    { work_state: 'commit' },
    [
      meta => ctx.patchouliStorage.create({
        meta,
        data: { type: 'event', id: 'work-event-1', value: { payload: 'first' } },
      }),
      meta => ctx.patchouliStorage.create({
        meta,
        data: { type: 'event', id: 'work-event-2', value: { payload: 'second' } },
      }),
    ],
  )
  assert.equal(workUnit.length, 2)
  assert.deepEqual(successfulCreateRequests[0]!.meta, {
    workspace: 'test',
    work_id: 'work-1',
    fixture_success: true,
  })
  assert.deepEqual(successfulCreateRequests[1]!.meta, {
    workspace: 'test',
    work_id: 'work-1',
    fixture_success: true,
    work_state: 'commit',
  })
  await assert.rejects(
    ctx.patchouliStorage.runWorkUnit({}, {}, []),
    /requires at least one mutation/,
  )
  await assert.rejects(
    ctx.patchouliStorage.runWorkUnit(
      { work_id: 'one' },
      { work_id: 'two' },
      [async () => undefined],
    ),
    /must not override base metadata/,
  )

  const events: ChangesEventParams[] = []
  const subscription = await ctx.patchouliStorage.subscribe({
    meta: { workspace: 'test' },
    data: { filter: { types: ['event'] } },
  }, event => {
    events.push(event)
  })
  assert.equal(subscription.data.subscription_id, 'subscription-1')
  assert.equal(subscription.data.cursor, 'cursor-0')
  assert.equal(events.length, 1)
  const firstEvent = events[0]
  assert.ok(firstEvent)
  assert.equal(firstEvent.meta.transaction_id, 'transaction-1')
  assert.equal(firstEvent.data.change.cursor, 'cursor-1')

  const unsubscribe = await ctx.patchouliStorage.unsubscribe({
    meta: {},
    data: { subscription_id: subscription.data.subscription_id },
  })
  assert.equal(unsubscribe.data.removed, true)
  assert.deepEqual(await subscription.closed, { kind: 'unsubscribed' })
  assert.equal(events.length, 1)

  await assert.rejects(
    ctx.patchouliStorage.create({
      meta: {},
      data: { type: 'event', id: 'event-1', value: { payload: 'hello' } },
    }),
    (error) => {
      assert.ok(error instanceof storage.PatchouliRpcError)
      assert.equal(error.method, 'patchouli.entity.create@1')
      assert.equal(error.code, -32006)
      assert.equal(error.reason, 'UNSUPPORTED_CAPABILITY')
      assert.deepEqual(error.data, { reason: 'UNSUPPORTED_CAPABILITY' })
      return true
    },
  )

  const handledSubscription = await ctx.patchouliStorage.subscribe({
    meta: { workspace: 'test' },
    data: {},
  }, () => {})
  const firstUnsubscribe = handledSubscription.unsubscribe()
  assert.equal(handledSubscription.unsubscribe(), firstUnsubscribe)
  await firstUnsubscribe
  assert.deepEqual(await handledSubscription.closed, { kind: 'unsubscribed' })
  assert.equal(unsubscribeRequests, 2)

  const disconnectedSubscription = await ctx.patchouliStorage.subscribe({
    meta: { workspace: 'test' },
    data: {},
  }, () => {})
  assert.ok(serverSocket)
  serverSocket.destroy()
  const disconnected = await disconnectedSubscription.closed
  assert.equal(disconnected.kind, 'connection-lost')
  assert.ok(disconnected.error instanceof Error)
})
