import assert from 'node:assert/strict'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'
import AgentRegistry, { agentEvents } from '@deepseek-ai/dsh-agent'
import { CallId, createUserMessage } from '@deepseek-ai/dsh-llm'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRuntime from '@deepseek-ai/dsh-tools'
import * as agentLoop from '../lib/agent-loop.js'
import * as patchouli from '../lib/index.js'

const SIGNAL = new AbortController().signal

async function mountConsumer(t) {
  const ctx = new Context()
  const fibers = [
    await ctx.plugin(AgentRegistry),
    await ctx.plugin(SystemPrompt),
    await ctx.plugin(ToolRuntime),
    await ctx.plugin(patchouli),
    await ctx.plugin(agentLoop),
  ]
  t.after(async () => {
    for (const fiber of fibers.reverse()) await fiber.dispose()
  })
  return ctx
}

function fakeAgent(cwd = '/workspace/patchouli') {
  return {
    session: {
      header: {
        id: 'session-1',
        cwd,
      },
    },
  }
}

test('registers update/retrieve tools and derives their scope from the agent', async (t) => {
  const ctx = await mountConsumer(t)
  const calls = []
  const dispose = ctx.patchouliMemory.register({
    id: 'fixture',
    async update(request, context) {
      calls.push(['update', request, context.signal])
      return { status: 'applied', receipt: 'u1' }
    },
    async retrieve(request, context) {
      calls.push(['retrieve', request, context.signal])
      return { items: [{ content: 'remembered result' }] }
    },
  })
  t.after(dispose)

  const agent = fakeAgent()
  assert.deepEqual(
    ctx.tools.schemas(agent).map(schema => schema.name).filter(name => name.startsWith('memory_')),
    ['memory_retrieve', 'memory_update'],
  )

  const retrieve = await ctx.tools.execute({
    callId: CallId('retrieve-1'),
    name: 'memory_retrieve',
    arguments: { query: ' prior work ', limit: 3 },
    agent,
    signal: SIGNAL,
  })
  assert.equal(retrieve.isError, false)
  assert.match(retrieve.content[0].text, /remembered result/)

  const update = await ctx.tools.execute({
    callId: CallId('update-1'),
    name: 'memory_update',
    arguments: { messages: [{ role: 'user', content: ' remember this ' }] },
    agent,
    signal: SIGNAL,
  })
  assert.equal(update.isError, false)
  assert.match(update.content[0].text, /\[fixture\] applied \(u1\)/)

  assert.deepEqual(calls, [
    ['retrieve', { scope: '/workspace/patchouli', query: 'prior work', limit: 3 }, SIGNAL],
    ['update', {
      scope: '/workspace/patchouli',
      messages: [{ role: 'user', content: 'remember this' }],
    }, SIGNAL],
  ])
})

test('retrieves once for admitted direct user input and injects recall context', async (t) => {
  const ctx = await mountConsumer(t)
  const requests = []
  const dispose = ctx.patchouliMemory.register({
    id: 'fixture',
    async update() {
      return { status: 'accepted' }
    },
    async retrieve(request, context) {
      requests.push([request, context.signal])
      return { items: [{ content: 'use the repository convention' }] }
    },
  })
  t.after(dispose)

  const agent = fakeAgent()
  const user = createUserMessage({
    content: [{ type: 'text', text: ' How should this be implemented? ' }],
    source: { kind: 'user' },
  })
  const decision = await agentEvents(ctx, agent).waterfall(
    'agent/pre-step',
    { messages: [user], turn: 1, step: 1, signal: SIGNAL },
    () => Promise.resolve({ kind: 'enter', messages: [user] }),
  )

  assert.equal(decision.kind, 'enter')
  assert.equal(decision.messages.length, 2)
  const recall = decision.messages[1]
  assert.deepEqual(recall.source, {
    kind: 'plugin',
    plugin: 'dsh-patchouli-agent-loop',
    form: 'recall',
  })
  assert.match(recall.content[0].text, /use the repository convention/)
  assert.deepEqual(requests, [[{
    scope: '/workspace/patchouli',
    query: 'How should this be implemented?',
    limit: 5,
  }, SIGNAL]])

  const continuation = createUserMessage({
    content: [{ type: 'text', text: 'tool result context' }],
    source: { kind: 'plugin', plugin: 'fixture-tool' },
  })
  await agentEvents(ctx, agent).waterfall(
    'agent/pre-step',
    { messages: [continuation], turn: 1, step: 2, signal: SIGNAL },
    () => Promise.resolve({ kind: 'enter', messages: [continuation] }),
  )
  await agentEvents(ctx, agent).waterfall(
    'agent/pre-step',
    { messages: [user], turn: 2, step: 1, signal: SIGNAL },
    () => Promise.resolve({ kind: 'reject' }),
  )
  const removed = await agentEvents(ctx, agent).waterfall(
    'agent/pre-step',
    { messages: [user], turn: 3, step: 1, signal: SIGNAL },
    () => Promise.resolve({ kind: 'enter', messages: [] }),
  )
  assert.deepEqual(removed, { kind: 'enter', messages: [] })
  assert.equal(requests.length, 1)
})
