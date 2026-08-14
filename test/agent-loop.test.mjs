import assert from 'node:assert/strict'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'
import AgentRegistry, { agentEvents } from '@deepseek-ai/dsh-agent'
import {
  CallId,
  createAssistantMessage,
  createToolResultMessage,
  createUserMessage,
} from '@deepseek-ai/dsh-llm'
import SessionStore from '@deepseek-ai/dsh-session'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRuntime from '@deepseek-ai/dsh-tools'
import * as agentLoop from '../lib/agent-loop.js'
import * as patchouli from '../lib/index.js'

const SIGNAL = new AbortController().signal

async function mountConsumer(t, config = {}) {
  const ctx = new Context()
  const fibers = []
  fibers.push(await ctx.plugin(SessionStore))
  fibers.push(await ctx.plugin(AgentRegistry))
  fibers.push(await ctx.plugin(SystemPrompt))
  fibers.push(await ctx.plugin(ToolRuntime))
  fibers.push(await ctx.plugin(patchouli))
  const consumer = await ctx.plugin(agentLoop, config)
  fibers.push(consumer)
  t.after(async () => {
    for (const fiber of fibers.reverse()) await fiber.dispose()
  })
  return { ctx, consumer }
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
  const { ctx } = await mountConsumer(t)
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
    ['retrieve', {
      scope: '/workspace/patchouli',
      query: 'prior work',
      limit: 3,
      metadata: {
        agentLoop: 'deepseek-official',
        trigger: 'manual-tool',
        sessionId: 'session-1',
      },
    }, SIGNAL],
    ['update', {
      scope: '/workspace/patchouli',
      messages: [{ role: 'user', content: 'remember this' }],
      metadata: {
        agentLoop: 'deepseek-official',
        trigger: 'manual-tool',
        sessionId: 'session-1',
      },
    }, SIGNAL],
  ])
})

test('retrieves once for admitted direct user input and injects recall context', async (t) => {
  const { ctx } = await mountConsumer(t)
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
    metadata: {
      agentLoop: 'deepseek-official',
      trigger: 'pre-step',
      sessionId: 'session-1',
      turn: 1,
      step: 1,
    },
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

test('submits visible text from a successfully committed turn when automatic update is enabled', async (t) => {
  const { ctx } = await mountConsumer(t, { autoUpdate: true })
  const calls = []
  const updated = Promise.withResolvers()
  const dispose = ctx.patchouliMemory.register({
    id: 'fixture',
    async update(request, context) {
      calls.push([request, context.signal])
      updated.resolve()
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
  })
  t.after(dispose)

  const session = ctx.sessions.create('session-turn', {
    meta: { cwd: '/workspace/patchouli' },
  })
  const callId = CallId('call-1')

  session.append('turn/start', { turn: 1 })
  session.append('step/start', { turn: 1, step: 1 })
  session.append('user/message', createUserMessage({
    content: [{ type: 'text', text: ' remember this decision ' }],
    source: { kind: 'user' },
  }), { surfaceOp: 'append' })
  session.append('user/message', createUserMessage({
    content: [{ type: 'text', text: 'recalled context must not be written back' }],
    source: { kind: 'plugin', plugin: 'fixture-recall', form: 'recall' },
  }), { surfaceOp: 'append' })
  session.append('assistant/message', {
    turn: 1,
    step: 1,
    message: createAssistantMessage({
      content: [
        { type: 'reasoning', text: 'private reasoning' },
        { type: 'text', text: ' I will inspect the code. ' },
        { type: 'tool-call', id: callId, name: 'read', arguments: '{}' },
      ],
      source: { provider: 'fixture', model: 'fixture' },
    }),
  }, { surfaceOp: 'append' })
  session.append('tool/result', {
    turn: 1,
    step: 1,
    message: createToolResultMessage({
      callId,
      content: [{ type: 'text', text: 'large untrusted tool output' }],
      isError: false,
    }),
  }, { surfaceOp: 'append' })
  session.append('step/end', { turn: 1, step: 1 })
  session.append('step/start', { turn: 1, step: 2 })
  session.append('assistant/message', {
    turn: 1,
    step: 2,
    message: createAssistantMessage({
      content: [{ type: 'text', text: ' Use the committed event boundary. ' }],
      source: { provider: 'fixture', model: 'fixture' },
    }),
  }, { surfaceOp: 'append' })
  session.append('step/end', { turn: 1, step: 2 })

  assert.equal(calls.length, 0)
  session.append('turn/end', { turn: 1, reason: { kind: 'completed' } })
  await updated.promise

  assert.equal(calls.length, 1)
  assert.deepEqual(calls[0][0], {
    scope: '/workspace/patchouli',
    messages: [
      { role: 'user', content: 'remember this decision' },
      { role: 'assistant', content: 'I will inspect the code.' },
      { role: 'assistant', content: 'Use the committed event boundary.' },
    ],
    metadata: {
      agentLoop: 'deepseek-official',
      trigger: 'turn-end',
      sessionId: 'session-turn',
      turn: 1,
      turnEndReason: 'completed',
    },
  })

  session.append('turn/start', { turn: 2 })
  session.append('user/message', createUserMessage({
    content: [{ type: 'text', text: 'cancelled input' }],
    source: { kind: 'user' },
  }), { surfaceOp: 'append' })
  session.append('turn/end', {
    turn: 2,
    reason: { kind: 'aborted', reason: { kind: 'user' } },
  })
  await Promise.resolve()
  assert.equal(calls.length, 1)
})

test('aborts and drains an admitted automatic update during consumer disposal', async (t) => {
  const { ctx, consumer } = await mountConsumer(t, { autoUpdate: true })
  const started = Promise.withResolvers()
  const release = Promise.withResolvers()
  const calls = []
  const dispose = ctx.patchouliMemory.register({
    id: 'fixture',
    async update(request, context) {
      calls.push(request)
      started.resolve(context.signal)
      await release.promise
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
  })
  t.after(dispose)

  const session = ctx.sessions.create('session-dispose', {
    meta: { cwd: '/workspace/patchouli' },
  })
  session.append('turn/start', { turn: 1 })
  session.append('user/message', createUserMessage({
    content: [{ type: 'text', text: 'first committed turn' }],
    source: { kind: 'user' },
  }), { surfaceOp: 'append' })
  session.append('turn/end', { turn: 1, reason: { kind: 'completed' } })

  const signal = await started.promise
  const aborted = new Promise(resolve => {
    if (signal.aborted) resolve()
    else signal.addEventListener('abort', resolve, { once: true })
  })
  const disposing = consumer.dispose()
  let disposed = false
  void disposing.then(() => { disposed = true })
  await aborted
  assert.equal(signal.aborted, true)
  assert.equal(disposed, false)

  session.append('turn/start', { turn: 2 })
  session.append('user/message', createUserMessage({
    content: [{ type: 'text', text: 'must not be admitted after disposal' }],
    source: { kind: 'user' },
  }), { surfaceOp: 'append' })
  session.append('turn/end', { turn: 2, reason: { kind: 'completed' } })

  release.resolve()
  await disposing
  await Promise.resolve()
  assert.equal(calls.length, 1)
})
