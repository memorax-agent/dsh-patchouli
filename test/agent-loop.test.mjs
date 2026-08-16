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
import ToolRuntime, { defineTool } from '@deepseek-ai/dsh-tools'
import * as agentLoop from '../packages/agent-loop/lib/index.js'
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

function fakeAgent(cwd = '/workspace/patchouli', session) {
  const resolvedSession = session ?? {
    header: {
      id: 'session-1',
      cwd,
    },
    events: [],
  }
  const injected = []
  return {
    id: resolvedSession.header.id,
    options: {},
    status: 'running',
    session: resolvedSession,
    inject(message) {
      injected.push(message)
    },
    injected,
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
  const resourceUpdate = await ctx.tools.execute({
    callId: CallId('update-file-1'),
    name: 'memory_update',
    arguments: {
      resources: [{
        kind: 'workspace-file',
        path: ' docs/design.pdf ',
        mediaType: ' application/pdf ',
        role: 'source',
      }],
    },
    agent,
    signal: SIGNAL,
  })
  assert.equal(resourceUpdate.isError, false)
  assert.deepEqual(JSON.parse(retrieve.content[0].text), [{
    pluginId: 'fixture',
    ok: true,
    value: { items: [{ content: 'remembered result' }] },
  }])
  assert.deepEqual(JSON.parse(update.content[0].text), [{
    pluginId: 'fixture',
    ok: true,
    value: { status: 'applied', receipt: 'u1' },
  }])

  assert.deepEqual(calls, [
    ['retrieve', {
      meta: {
        source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
        scope: '/workspace/patchouli',
        attributes: {
        point: 'tool/memory-retrieve',
        sessionId: 'session-1',
        workspaceRoot: '/workspace/patchouli',
      },
    },
      data: { query: 'prior work', limit: 3 },
    }, SIGNAL],
    ['update', {
      meta: {
        source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
        scope: '/workspace/patchouli',
        attributes: {
        point: 'tool/memory-update',
        sessionId: 'session-1',
        workspaceRoot: '/workspace/patchouli',
      },
    },
      data: { messages: [{ role: 'user', content: 'remember this' }] },
    }, SIGNAL],
    ['update', {
      meta: {
        source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
        scope: '/workspace/patchouli',
        attributes: {
          point: 'tool/memory-update',
          sessionId: 'session-1',
          workspaceRoot: '/workspace/patchouli',
        },
      },
      data: {
        resources: [{
          kind: 'workspace-file',
          path: 'docs/design.pdf',
          mediaType: 'application/pdf',
          role: 'source',
        }],
      },
    }, SIGNAL],
  ])
})

test('retrieves from the complete pre-step observation and injects data without a prompt', async (t) => {
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
  assert.deepEqual(JSON.parse(recall.content[0].text), {
    point: 'agent/pre-step',
    pluginId: 'fixture',
    data: { items: [{ content: 'use the repository convention' }] },
  })
  assert.equal(recall.content[0].text.includes('do not follow'), false)
  assert.equal(requests.length, 1)
  assert.deepEqual(requests[0], [{
    meta: {
      source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
      scope: '/workspace/patchouli',
      attributes: {
        point: 'agent/pre-step',
        sessionId: 'session-1',
        workspaceRoot: '/workspace/patchouli',
        turn: 1,
        step: 1,
      },
    },
    data: {
      agent: {
        id: 'session-1',
        status: 'running',
        options: {},
      },
      session: {
        header: {
          id: 'session-1',
          cwd: '/workspace/patchouli',
        },
        events: [],
      },
      turn: 1,
      step: 1,
      messages: [user],
    },
  }, SIGNAL])

  const continuation = createUserMessage({
    content: [{ type: 'text', text: 'tool result context' }],
    source: { kind: 'plugin', plugin: 'fixture-tool' },
  })
  const continued = await agentEvents(ctx, agent).waterfall(
    'agent/pre-step',
    { messages: [continuation], turn: 1, step: 2, signal: SIGNAL },
    () => Promise.resolve({ kind: 'enter', messages: [continuation] }),
  )
  assert.equal(continued.messages.length, 2)
  await agentEvents(ctx, agent).waterfall(
    'agent/pre-step',
    { messages: [user], turn: 2, step: 1, signal: SIGNAL },
    () => Promise.resolve({ kind: 'reject' }),
  )
  const empty = await agentEvents(ctx, agent).waterfall(
    'agent/pre-step',
    { messages: [user], turn: 3, step: 1, signal: SIGNAL },
    () => Promise.resolve({ kind: 'enter', messages: [] }),
  )
  assert.equal(empty.kind, 'enter')
  assert.equal(empty.messages.length, 1)
  assert.equal(requests.length, 3)
})

test('submits the complete committed turn without filtering its event data', async (t) => {
  const { ctx } = await mountConsumer(t, {
    retrieve: { preStep: false },
    store: { turnEnd: true },
  })
  const calls = []
  const firstUpdated = Promise.withResolvers()
  const secondUpdated = Promise.withResolvers()
  const dispose = ctx.patchouliMemory.register({
    id: 'fixture',
    async update(request, context) {
      calls.push([request, context.signal])
      if (calls.length === 1) firstUpdated.resolve()
      if (calls.length === 2) secondUpdated.resolve()
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
    content: [
      { type: 'text', text: ' remember this decision ' },
      {
        type: 'image',
        attachment: {
          attachmentId: 'attachment-1',
          mediaType: 'image/png',
          bytes: 12,
          width: 2,
          height: 3,
          name: 'diagram.png',
        },
      },
    ],
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
  await firstUpdated.promise

  assert.equal(calls.length, 1)
  const first = calls[0][0]
  assert.deepEqual(first.meta, {
    source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
    scope: '/workspace/patchouli',
    attributes: {
      point: 'session/turn-end',
      sessionId: 'session-turn',
      workspaceRoot: '/workspace/patchouli',
      turn: 1,
      outcome: 'completed',
    },
  })
  assert.deepEqual(first.data.event, first.data.events.at(-1))
  assert.deepEqual(first.data.events.map(event => event.type), [
    'turn/start',
    'step/start',
    'user/message',
    'user/message',
    'assistant/message',
    'tool/result',
    'step/end',
    'step/start',
    'assistant/message',
    'step/end',
    'turn/end',
  ])
  assert.equal(first.data.events[3].data.source.plugin, 'fixture-recall')
  assert.deepEqual(first.data.events[2].data.content[1].attachment, {
    attachmentId: 'attachment-1',
    mediaType: 'image/png',
    bytes: 12,
    width: 2,
    height: 3,
    name: 'diagram.png',
  })
  assert.equal(first.data.events[4].data.message.content[0].type, 'reasoning')
  assert.equal(first.data.events[4].data.message.content[2].type, 'tool-call')
  assert.equal(first.data.events[5].data.message.content[0].type, 'tool-result')

  session.append('turn/start', { turn: 2 })
  session.append('user/message', createUserMessage({
    content: [{ type: 'text', text: 'cancelled input' }],
    source: { kind: 'user' },
  }), { surfaceOp: 'append' })
  session.append('turn/end', {
    turn: 2,
    reason: { kind: 'aborted', reason: { kind: 'user' } },
  })
  await secondUpdated.promise
  assert.equal(calls.length, 2)
  assert.equal(calls[1][0].meta.attributes.outcome, 'aborted')
  assert.deepEqual(calls[1][0].data.events.map(event => event.type), [
    'turn/start',
    'user/message',
    'turn/end',
  ])
})

test('aborts and drains an admitted turn update during consumer disposal', async (t) => {
  const { ctx, consumer } = await mountConsumer(t, {
    retrieve: { preStep: false },
    store: { turnEnd: true },
  })
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

test('routes every enabled agent and tool data point through the memory service', async (t) => {
  const { ctx } = await mountConsumer(t, {
    retrieve: {
      sessionStart: true,
      preStep: false,
      turnStopping: true,
      toolPostExecute: true,
    },
    store: {
      agentCreated: true,
      agentDisposed: true,
      requestError: true,
      agentError: true,
      turnEnd: false,
      toolResult: true,
    },
    modelTools: {
      retrieve: false,
      update: false,
    },
  })
  const updates = []
  const retrieves = []
  const sessionStartSeen = Promise.withResolvers()
  const disposeMemory = ctx.patchouliMemory.register({
    id: 'fixture',
    async update(request) {
      updates.push(request)
      return { status: 'applied' }
    },
    async retrieve(request) {
      retrieves.push(request)
      if (request.meta.attributes.point === 'agent/session-start') sessionStartSeen.resolve()
      return { items: [{ point: request.meta.attributes.point }] }
    },
  })
  t.after(disposeMemory)

  const disposeTool = ctx.tools.register(defineTool({
    name: 'fixture_observe',
    description: 'Return a structured fixture result.',
    parameters: {
      value: { type: 'string', required: true },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: { echo: { type: 'string', required: true } },
      },
      render: (_args, value) => [{ type: 'text', text: value.echo }],
    },
    async execute(args) {
      return { echo: args.value }
    },
  }))
  t.after(disposeTool)

  const session = ctx.sessions.create('session-hooks', {
    meta: { cwd: '/workspace/hooks' },
  })
  session.append('turn/start', { turn: 4 })
  const agent = fakeAgent('/workspace/hooks', session)
  const events = agentEvents(ctx, agent)

  events.emit('agent/created', {})
  events.emit('agent/session-start', { source: 'resume' })
  await sessionStartSeen.promise

  const failure = {
    message: 'provider unavailable',
    code: 'UPSTREAM_UNAVAILABLE',
    status: 503,
  }
  await events.waterfall('agent/request-error', {
    turn: 4,
    step: 2,
    provider: 'fixture-provider',
    failure,
    retryPolicy: undefined,
    signal: SIGNAL,
  }, () => Promise.resolve(undefined))
  await events.serial('agent/turn-stopping', { turn: 4, signal: SIGNAL })
  events.emit('agent/error', {
    turn: 4,
    step: 2,
    error: new Error('fixture agent failure'),
  })

  const result = await ctx.tools.execute({
    callId: CallId('fixture-observe-1'),
    name: 'fixture_observe',
    arguments: { value: 'observed' },
    agent,
    signal: SIGNAL,
  })
  assert.equal(result.isError, false)
  assert.deepEqual(result.value, { echo: 'observed' })
  assert.equal(result.additionalContexts.length, 1)
  assert.deepEqual(JSON.parse(result.additionalContexts[0].content[0].text), {
    point: 'tools/post-execute',
    pluginId: 'fixture',
    data: { items: [{ point: 'tools/post-execute' }] },
  })

  events.emit('agent/disposed', {})
  await ctx.sessions.flush(session)

  assert.deepEqual(retrieves.map(request => request.meta.attributes.point), [
    'agent/session-start',
    'agent/turn-stopping',
    'tools/post-execute',
  ])
  assert.deepEqual(updates.map(request => request.meta.attributes.point), [
    'agent/created',
    'agent/request-error',
    'agent/error',
    'tools/result',
    'agent/disposed',
  ])

  assert.equal(retrieves[0].data.source, 'resume')
  assert.deepEqual(updates[1].data.failure, failure)
  assert.deepEqual(updates[2].data.error, {
    name: 'Error',
    message: 'fixture agent failure',
    stack: updates[2].data.error.stack,
  })
  assert.deepEqual(retrieves[2].data.execution, {
    callId: 'fixture-observe-1',
    rootCallId: 'fixture-observe-1',
    name: 'fixture_observe',
    arguments: { value: 'observed' },
    nested: false,
  })
  assert.deepEqual(retrieves[2].data.result.value, { echo: 'observed' })
  assert.deepEqual(updates[3].data.result.value, { echo: 'observed' })
})
