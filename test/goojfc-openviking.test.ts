import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import test from 'node:test'

import {
  createOpenVikingAdapter,
  type OpenVikingClient,
  type OpenVikingRuntime,
} from '../lib/goojfc/adapters/openviking.js'

const require = createRequire(import.meta.url)
const patches = require('../patches/openviking.patch.cjs') as Array<{
  id: string
  target: { package: string; version: string; file: string }
  select: string
  expect: number
}>

const meta = {
  source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
  scope: '/workspace/project',
  attributes: {
    point: 'tool/memory-retrieve',
    sessionId: 'session/1',
    workspaceRoot: '/workspace/project',
  },
} as const

test('pins the OpenViking Harmony seam and disables native automatic hooks', () => {
  assert.deepEqual(patches.map(patch => patch.id), [
    'openviking-require-goojfc',
    'openviking-expose-native-service',
    'openviking-disable-native-automatic-hooks',
  ])
  assert.ok(patches.every(patch => patch.target.package === '@openviking/dsh-memory-plugin'))
  assert.ok(patches.every(patch => patch.target.version === '0.1.0'))
  assert.deepEqual(patches.map(patch => patch.expect), [1, 1, 5])
})

test('routes retrieval through OpenViking runtime recall', async () => {
  const calls: unknown[] = []
  const result = {
    role: 'user',
    content: [{ type: 'text', text: '<openviking-context>SQLite</openviking-context>' }],
  }
  const adapter = createOpenVikingAdapter(runtime(calls, {
    async recallMessage(agent, messages) {
      calls.push(['recall', agent, messages])
      return result
    },
  }), client())

  assert.equal(await adapter.retrieve({
    meta,
    data: { query: 'where is it?' },
  }, {}), result)
  assert.deepEqual(calls, [[
    'recall',
    { session: { id: 'session/1', header: { cwd: '/workspace/project' } } },
    [{
      role: 'user',
      content: [{ type: 'text', text: 'where is it?' }],
      source: { kind: 'user' },
    }],
  ]])
})

test('uses the process cwd when the session has no workspace root', async () => {
  const calls: unknown[] = []
  const adapter = createOpenVikingAdapter(runtime(calls, {
    async recallMessage(agent) {
      calls.push(['recall', agent])
      return null
    },
  }), client())
  const { workspaceRoot: _workspaceRoot, ...attributes } = meta.attributes

  assert.equal(await adapter.retrieve({
    meta: { ...meta, attributes },
    data: { query: 'database' },
  }, {}), null)
  assert.deepEqual(calls, [[
    'recall',
    { session: { id: 'session/1', header: { cwd: process.cwd() } } },
  ]])
})

test('routes explicit messages through capture queue and flush', async () => {
  const calls: unknown[] = []
  const adapter = createOpenVikingAdapter(runtime(calls), client({
    async addResource(path, reason, peerId) {
      calls.push(['resource', path, reason, peerId])
      return { uri: 'viking://resources/readme' }
    },
  }))

  const value = await adapter.update({
    meta: { ...meta, attributes: { ...meta.attributes, point: 'tool/memory-update' } },
    data: {
      messages: [{ role: 'user', content: 'remember this' }],
      resources: [{ path: 'README.md', reason: 'project guide' }],
    },
  }, {})

  assert.deepEqual(value, {
    captured: 1,
    resources: [{ uri: 'viking://resources/readme' }],
  })
  assert.equal((calls[1] as unknown[])[0], 'capture')
  assert.deepEqual(calls.at(-2), ['flush', 'session/1'])
  assert.deepEqual(calls.at(-1), [
    'resource', 'README.md', 'project guide', 'peer:/workspace/project',
  ])
})

test('replays coordinated lifecycle and turn events through the native runtime', async () => {
  const calls: unknown[] = []
  const adapter = createOpenVikingAdapter(runtime(calls, {
    async profileMessage(agent) {
      calls.push(['profile', agent])
      return 'profile context'
    },
  }), client())
  const created = { ...meta, attributes: { ...meta.attributes, point: 'agent/created' } }
  const sessionStart = {
    ...meta,
    attributes: { ...meta.attributes, point: 'agent/session-start' },
  }
  const turnEnd = { ...meta, attributes: { ...meta.attributes, point: 'session/turn-end' } }
  const disposed = { ...meta, attributes: { ...meta.attributes, point: 'agent/disposed' } }
  const event = { type: 'turn/end', seq: 4, data: { turn: 1 } }

  assert.deepEqual(await adapter.update({ meta: created, data: {} }, {}), { initialized: true })
  assert.equal(await adapter.retrieve({ meta: sessionStart, data: {} }, {}), 'profile context')
  assert.deepEqual(await adapter.update({ meta: turnEnd, data: { events: [event] } }, {}), {
    captured: 1,
    resources: [],
  })
  assert.deepEqual(await adapter.update({ meta: disposed, data: {} }, {}), { disposed: true })
  assert.ok(calls.some(call => (call as unknown[])[0] === 'maybeCommit'))
  assert.deepEqual(calls.at(-1), ['dispose', 'session/1'])
})

test('accepts only the coordinated Agent Loop points it implements', () => {
  const adapter = createOpenVikingAdapter(runtime([]), client())
  assert.equal(adapter.filter?.({
    operation: 'update',
    meta: { ...meta, attributes: { ...meta.attributes, point: 'session/turn-end' } },
  }), true)
  assert.equal(adapter.filter?.({
    operation: 'update',
    meta: { ...meta, attributes: { ...meta.attributes, point: 'agent/error' } },
  }), false)
  assert.equal(adapter.filter?.({
    operation: 'retrieve',
    meta,
  }), true)
})

function runtime(
  calls: unknown[],
  overrides: Partial<OpenVikingRuntime> = {},
): OpenVikingRuntime {
  return {
    async initialize(agent) { calls.push(['initialize', agent]) },
    stateFor(session) {
      return { config: { peerId: `peer:${session.header.cwd}` } }
    },
    capture(session, event) { calls.push(['capture', session, event]) },
    maybeCommit(session, event) { calls.push(['maybeCommit', session, event]) },
    async flush(session) { calls.push(['flush', session.id]) },
    async dispose(session) { calls.push(['dispose', session.id]) },
    async profileMessage() { return null },
    async recallMessage() { return null },
    ...overrides,
  } as OpenVikingRuntime
}

function client(overrides: Partial<OpenVikingClient> = {}): OpenVikingClient {
  return {
    async addResource() { return null },
    ...overrides,
  }
}
