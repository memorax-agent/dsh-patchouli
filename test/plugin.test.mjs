import assert from 'node:assert/strict'
import { randomUUID } from 'node:crypto'
import { mkdtemp, rm } from 'node:fs/promises'
import { createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'

import * as plugin from '../lib/index.js'

test('registers a daemon service and completes the control handshake', async (t) => {
  const directory = await mkdtemp(join(tmpdir(), 'patchouli-plugin-'))
  const endpoint = process.platform === 'win32'
    ? String.raw`\\.\pipe\patchouli-plugin-${process.pid}-${randomUUID()}`
    : join(directory, 'patchouli.sock')
  const server = createServer((socket) => {
    let buffer = ''
    socket.setEncoding('utf8')
    socket.on('data', (chunk) => {
      buffer += chunk
      while (true) {
        const newline = buffer.indexOf('\n')
        if (newline < 0) return
        const request = JSON.parse(buffer.slice(0, newline))
        buffer = buffer.slice(newline + 1)
        if (request.method === 'patchouli.entity.create@1') {
          socket.write(JSON.stringify({
            jsonrpc: '2.0',
            id: request.id,
            error: {
              code: -32006,
              message: 'entity create is not implemented by the backend engine',
              data: { reason: 'UNSUPPORTED_CAPABILITY' },
            },
          }) + '\n')
          continue
        }
        const result = request.method === 'patchouli.protocol.handshake@1'
          ? {
              protocol_version: 1,
              server: { version: '0.1.0', cluster_id: 'test', node_id: 'test' },
              capabilities: ['control.status', 'control.shutdown'],
              limits: {
                max_request_bytes: 1_048_576,
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
                pid: process.pid,
                started_at_unix_ms: 1,
                active_connections: 1,
              },
            }
        socket.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\n')
      }
    })
  })

  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(endpoint, resolve)
  })
  const ctx = new Context()
  t.after(async () => {
    await ctx.fiber.dispose()
    await new Promise(resolve => server.close(resolve))
    await rm(directory, { recursive: true })
  })

  assert.equal(plugin.name, 'dsh-patchouli')
  assert.deepEqual(plugin.inject, [])
  await ctx.plugin(plugin, {
    endpoint,
    command: 'patchouli',
    autoStart: false,
    startupTimeoutMs: 100,
  })

  assert.equal(ctx.patchouli.server?.server.node_id, 'test')
  const status = await ctx.patchouli.status()
  assert.equal(status.data.ready, true)
  assert.equal(status.data.provider, 'sqlite')
  assert.equal(status.data.pid, process.pid)
  await assert.rejects(
    ctx.patchouli.create({
      meta: {},
      data: { type: 'event', id: 'event-1', value: { payload: 'hello' } },
    }),
    /RPC -32006: entity create is not implemented/,
  )
})
