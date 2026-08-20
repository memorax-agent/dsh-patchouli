import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import test from 'node:test'

import { createMemosAdapter } from '../lib/goojfc/adapters/memos.js'

const require = createRequire(import.meta.url)
const patches = require('../patches/memos.patch.cjs') as Array<{
  id: string
  target: { package: string; version: string; file: string }
  select: string
  expect: number
  apply: unknown
}>

const meta = {
  source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
  scope: '/workspace/patchouli',
  requestId: 'request-1',
  attributes: {
    point: 'tool/memory-retrieve',
    sessionId: 'session-1',
    workspaceRoot: '/workspace/patchouli',
  },
} as const

const noopBridge = {
  onSessionEvent() {},
  async flush() {},
  async closeSession() {},
}

test('pins the MemOS seam and disables its native automatic hooks', () => {
  assert.deepEqual(patches.map(patch => patch.id), [
    'memos-require-goojfc',
    'memos-disable-native-recall',
    'memos-disable-native-capture',
    'memos-disable-native-dispose',
    'memos-disable-native-guidance',
    'memos-disable-native-tools',
    'memos-provide-bridge',
    'memos-bind-bridge',
  ])
  assert.ok(patches.every(patch => patch.expect === 1))
  assert.ok(patches.every(patch => patch.target.version === '2.0.16-beta.1'))
  const provide = patches.find(patch => patch.id === 'memos-provide-bridge')
  const bind = patches.find(patch => patch.id === 'memos-bind-bridge')
  assert.doesNotMatch(String(provide?.apply), /ctx\.on/)
  assert.match(String(provide?.apply), /ctx\.provide\("goojfcMemos"/)
  assert.match(String(bind?.apply), /goojfcMemosDelegate = ctx\.patchouliGoojfc\.createMemosAdapter/)
  assert.doesNotMatch(String(bind?.apply), /ctx\.provide/)
})

test('maps explicit retrieval to MemOS with a hard deadline envelope', async () => {
  const calls: unknown[] = []
  const result = { hits: [{ snippet: 'prior work' }], injectedContext: 'prior work' }
  const core = {
    async prepareTurn() { throw new Error('unexpected prepareTurn') },
    async onTurnEnd() { throw new Error('unexpected onTurnEnd') },
    async searchMemory(input: unknown, execution: unknown) {
      calls.push(['search', input, execution])
      return result
    },
  }
  const routes: unknown[] = []
  const adapter = createMemosAdapter(core, {
    profileId: 'default',
    recallEnabled: true,
    searchTimeoutMs: 3_000,
    bridge: noopBridge,
    now: () => 10_000,
    runWithLlmRoute(route, operation) {
      routes.push(route)
      return operation()
    },
  })
  const signal = new AbortController().signal

  assert.equal(await adapter.retrieve({
    meta,
    data: {
      query: 'prior work',
      limit: 4,
      agent: { options: { provider: 'openai', model: 'gpt-test' } },
    },
  }, { signal }), result)
  assert.deepEqual(routes, [{
    provider: 'openai', model: 'gpt-test', sessionId: 'session-1',
  }])
  assert.deepEqual(calls, [['search', {
    agent: 'deepseek-harness',
    namespace: {
      agentKind: 'deepseek-harness',
      profileId: 'default',
      profileLabel: 'default',
      workspacePath: '/workspace/patchouli',
      sessionKey: 'session-1',
    },
    sessionId: 'session-1',
    query: 'prior work',
    reason: 'tool_driven',
    contextHints: {
      patchouliPoint: 'tool/memory-retrieve',
      patchouliSource: meta.source,
    },
    deadlineAt: 13_000,
    llmFilterMalformedRetries: 0,
    topK: { tier1: 4, tier2: 4, tier3: 4 },
  }, { signal, foreground: true }]])
})

test('maps explicit messages through the MemOS turn lifecycle', async () => {
  const calls: unknown[] = []
  const core = {
    async prepareTurn(input: unknown) {
      calls.push(['prepare', input])
      return { sessionId: 'session-1', episodeId: 'episode-1' }
    },
    async onTurnEnd(input: unknown) {
      calls.push(['end', input])
      return { stored: true }
    },
    async searchMemory() { throw new Error('unexpected searchMemory') },
  }
  const adapter = createMemosAdapter(core, {
    profileId: 'work', recallEnabled: true, searchTimeoutMs: 3_000,
    bridge: noopBridge, now: () => 999,
  })
  const value = await adapter.update({
    meta: { ...meta, attributes: { ...meta.attributes, point: 'tool/memory-update' } },
    data: {
      messages: [
        { role: 'user', content: 'Remember SQLite' },
        { role: 'assistant', content: 'Recorded' },
      ],
    },
  }, {})

  assert.deepEqual(value, { stored: true })
  assert.equal(calls.length, 2)
  assert.match(JSON.stringify(calls[0]), /Remember SQLite/)
  assert.match(JSON.stringify(calls[1]), /Recorded/)
})

test('replays coordinated observations through one stable native session', async () => {
  const observed: unknown[] = []
  let closed: unknown
  const core = {
    async prepareTurn() { throw new Error('unexpected') },
    async onTurnEnd() { throw new Error('unexpected') },
    async searchMemory() { throw new Error('unexpected') },
  }
  const adapter = createMemosAdapter(core, {
    profileId: 'default', recallEnabled: true, searchTimeoutMs: 3_000,
    bridge: {
      onSessionEvent(session, event) { observed.push([session, event]) },
      async flush() {},
      async closeSession(session) { closed = session },
    },
  })
  assert.equal(adapter.filter?.({
    operation: 'update',
    meta: { ...meta, attributes: { ...meta.attributes, point: 'session/turn-end' } },
  }), true)
  const turnMeta = {
    ...meta,
    attributes: { ...meta.attributes, point: 'session/turn-end' },
  }
  await adapter.update({
    meta: turnMeta,
    data: {
      session: { header: { id: 'session-1', cwd: '/workspace/patchouli' } },
      events: [{ type: 'user/message', data: { content: 'remember' } }],
    },
  }, {})
  await adapter.update({
    meta: { ...turnMeta, attributes: { ...turnMeta.attributes, point: 'agent/disposed' } },
    data: {},
  }, {})
  assert.equal(observed.length, 1)
  assert.equal(closed, (observed[0] as unknown[])[0])
})

test('uses the active Agent preset and turn-start retrieval semantics', async () => {
  const searches: unknown[] = []
  const adapter = createMemosAdapter({
    async prepareTurn() { throw new Error('unexpected') },
    async onTurnEnd() { throw new Error('unexpected') },
    async searchMemory(input) {
      searches.push(input)
      return { hits: [] }
    },
  }, {
    profileId: 'configured', recallEnabled: true, searchTimeoutMs: 3_000,
    bridge: noopBridge,
  })

  await adapter.retrieve({
    meta: { ...meta, attributes: { ...meta.attributes, point: 'agent/pre-step', step: 1 } },
    data: {
      query: 'database',
      profileId: 'explicit',
      session: { header: { agentPreset: 'coding-preset' } },
    },
  }, {})

  assert.equal((searches[0] as { reason: string }).reason, 'turn_start')
  assert.equal(
    (searches[0] as { namespace: { profileId: string } }).namespace.profileId,
    'coding-preset',
  )
})

test('captures the native route from turn-end observation without pre-step recall', async () => {
  let replayedSession: { requestHeader(): unknown } | undefined
  const adapter = createMemosAdapter({
    async prepareTurn() { throw new Error('unexpected') },
    async onTurnEnd() { throw new Error('unexpected') },
    async searchMemory() { throw new Error('unexpected') },
  }, {
    profileId: 'default', recallEnabled: false, searchTimeoutMs: 3_000,
    bridge: {
      onSessionEvent(session) { replayedSession = session },
      async flush() {},
      async closeSession() {},
    },
  })

  assert.equal(await adapter.retrieve({
    meta,
    data: { query: 'ignored' },
  }, {}), null)
  await adapter.update({
    meta: { ...meta, attributes: { ...meta.attributes, point: 'session/turn-end' } },
    data: {
      agent: {
        id: 'agent-1',
        options: {
          provider: 'native-provider',
          model: 'native-model',
        },
      },
      session: { header: { agentPreset: 'preset' } },
      events: [{ type: 'turn/end', data: { turn: 1 } }],
    },
  }, {})

  assert.deepEqual(replayedSession?.requestHeader(), {
    config: {
      provider: 'native-provider',
      model: 'native-model',
      sessionId: 'session-1',
    },
  })
})
