import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import test from 'node:test'

import ts from 'typescript'

import { createHindsightAdapter } from '../lib/goojfc/adapters/hindsight.js'

const require = createRequire(import.meta.url)

interface SourcePatch {
  readonly id: string
  readonly target: {
    readonly package: string
    readonly version: string
    readonly files: readonly string[]
  }
  readonly select: string
  readonly expect: number
  apply(context: {
    node: ts.Node
    sourceFile: ts.SourceFile
    edit: {
      appendLeft(position: number, content: string): void
      appendRight(position: number, content: string): void
      overwrite(start: number, end: number, content: string): void
      remove(start: number, end: number): void
    }
    ts: typeof ts
  }): void
}

const patches = require('../patches/hindsight.patch.cjs') as SourcePatch[]

const targetExcerpt = `
var inject = ["agents"];
function apply(ctx) {
  const hooks = createDshHooks(workspaceForAgent);
  ctx.on("agent/session-start", hooks.sessionStart);
  ctx.on("agent/pre-step", hooks.preStep, { prepend: true });
  ctx.on("agent/turn-stopping", hooks.turnStopping);
  ctx.on("agent/disposed", hooks.disposed);
  ctx.inject(["tools"], (toolCtx) => {
    registerTools(toolCtx, process.cwd());
  });
}
`

function matchingNodes(patch: SourcePatch, sourceFile: ts.SourceFile): ts.Node[] {
  const matches: ts.Node[] = []
  const visit = (node: ts.Node): void => {
    if (
      patch.id === 'goojfc-hindsight-order'
      && ts.isVariableDeclaration(node)
      && ts.isIdentifier(node.name)
      && node.name.text === 'inject'
      && node.initializer !== undefined
      && ts.isArrayLiteralExpression(node.initializer)
    ) {
      matches.push(node.initializer)
    }
    if (
      patch.id === 'goojfc-hindsight-service'
      && ts.isVariableDeclaration(node)
      && ts.isIdentifier(node.name)
      && node.name.text === 'hooks'
    ) {
      matches.push(node)
    }
    if (
      patch.id === 'goojfc-hindsight-disable-native-hooks'
      && ts.isCallExpression(node)
      && ts.isPropertyAccessExpression(node.expression)
      && ts.isIdentifier(node.expression.expression)
      && node.expression.expression.text === 'ctx'
      && node.expression.name.text === 'on'
    ) {
      matches.push(node)
    }
    ts.forEachChild(node, visit)
  }
  visit(sourceFile)
  return matches
}

function applyPatches(source: string): string {
  let output = source
  for (const patch of patches) {
    const sourceFile = ts.createSourceFile(
      'dist/dsh.js',
      output,
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.JS,
    )
    const nodes = matchingNodes(patch, sourceFile)
    assert.equal(nodes.length, patch.expect)
    const edits: Array<{ start: number, end: number, content: string }> = []
    for (const node of nodes) {
      patch.apply({
        node,
        sourceFile,
        edit: {
          appendLeft(position, content) { edits.push({ start: position, end: position, content }) },
          appendRight(position, content) { edits.push({ start: position, end: position, content }) },
          overwrite(start, end, content) { edits.push({ start, end, content }) },
          remove(start, end) { edits.push({ start, end, content: '' }) },
        },
        ts,
      })
    }
    assert.ok(edits.length > 0)
    for (const edit of edits.sort((a, b) => b.start - a.start || b.end - a.end)) {
      output = output.slice(0, edit.start) + edit.content + output.slice(edit.end)
    }
  }
  return output
}

test('pins the Hindsight patch and waits for the GOOJFC registrar marker', () => {
  assert.deepEqual(patches.map(({ id, target, expect }) => ({ id, target, expect })), [
    {
      id: 'goojfc-hindsight-order',
      target: {
        package: '@vectorize-io/hindsight-coding-agents',
        version: '0.3.4',
        files: ['dist/dsh.js'],
      },
      expect: 1,
    },
    {
      id: 'goojfc-hindsight-disable-native-hooks',
      target: {
        package: '@vectorize-io/hindsight-coding-agents',
        version: '0.3.4',
        files: ['dist/dsh.js'],
      },
      expect: 4,
    },
    {
      id: 'goojfc-hindsight-service',
      target: {
        package: '@vectorize-io/hindsight-coding-agents',
        version: '0.3.4',
        files: ['dist/dsh.js'],
      },
      expect: 1,
    },
  ])

  const transformed = applyPatches(targetExcerpt)
  assert.match(transformed, /var inject = \["agents", "patchouliGoojfc"\]/)
  assert.doesNotMatch(transformed, /ctx\.on\("agent\//)
  assert.doesNotMatch(transformed, /ctx\.on\("session\//)
  assert.match(transformed, /workspace\.core\.onTranscript\(sessionId, readDshEvents\(events\)\)/)
  assert.match(transformed, /ctx\.provide\("goojfcHindsight"/)
  assert.match(transformed, /ctx\.patchouliGoojfc\.createHindsightAdapter/)
  assert.doesNotMatch(transformed, /dsh-patchouli\/goojfc/)
  assert.match(transformed, /ctx\.inject\(\["tools"\]/)
})

test('bridges Patchouli calls to Hindsight retain and reflect', async () => {
  const retainCalls: unknown[][] = []
  const reflectCalls: unknown[][] = []
  const workspace = {
    root: '/workspace/project',
    core: {
      bankId: 'coding-agent::project',
      cfg: { reflectTimeoutMs: 90_000 },
      client: {
        async retain(...args: unknown[]) {
          retainCalls.push(args)
        },
        async reflect(...args: unknown[]) {
          reflectCalls.push(args)
          return 'The project previously chose SQLite.'
        },
      },
    },
  }
  const service = createHindsightAdapter({
    harness: 'dsh',
    maxReflectTimeoutMs: 25_000,
    workspaceFor(root) {
      assert.equal(root, '/workspace/project')
      return workspace
    },
    ensureSeeded() {},
    async retainTranscript() { throw new Error('unexpected session retention') },
    readEvents: () => [],
    retainStamp: () => ({
      tags: ['configured'],
      metadata: { tenant: 'fixture' },
    }),
    reflectQuery: query => `reflect:${query}`,
    operationId: value => `uuid:${value}`,
  })

  assert.equal(service.id, 'hindsight')

  const meta = {
    source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
    scope: 'fallback-scope',
    requestId: 'request-1',
    attributes: {
      workspaceRoot: '/workspace/project',
      sessionId: 'session-1',
    },
  }
  assert.deepEqual(await service.update({
    meta,
    data: {
      messages: [
        { role: 'user', content: ' remember this ' },
        { role: 'assistant', content: 'recorded' },
      ],
    },
  }, {}), {
    accepted: true,
    bankId: 'coding-agent::project',
    documentId: 'patchouli:request-1',
  })
  assert.deepEqual(retainCalls, [[
    '{"role":"user","content":"remember this"}\n{"role":"assistant","content":"recorded"}',
    'coding agent session',
    'patchouli:request-1',
    ['configured', 'source:chat', 'harness:dsh'],
    'conversation',
    {
      metadata: {
        tenant: 'fixture',
        source: 'patchouli',
        source_type: 'agent-loop',
        source_id: 'dsh-patchouli-agent-loop',
        scope: 'fallback-scope',
      },
      operationId: 'uuid:coding-agent::project\npatchouli:request-1\n'
        + '{"role":"user","content":"remember this"}\n'
        + '{"role":"assistant","content":"recorded"}',
    },
  ]])

  assert.deepEqual(await service.retrieve({
    meta,
    data: {
      messages: [{
        role: 'user',
        source: { kind: 'user' },
        content: [{ type: 'text', text: ' prior choice ' }],
      }],
    },
  }, {}), { text: 'The project previously chose SQLite.' })
  assert.deepEqual(reflectCalls, [[
    'reflect:prior choice',
    { budget: 'low', timeoutMs: 25_000 },
  ]])
})

test('routes coordinated recall and native full-session retention through Hindsight', async () => {
  const calls: unknown[] = []
  const workspace = {
    root: '/workspace/project',
    core: {
      bankId: 'coding-agent::project',
      cfg: { reflectTimeoutMs: 10_000 },
      client: {
        async retain(...args: unknown[]) { calls.push(['retain', ...args]) },
        async reflect() { throw new Error('unexpected reflect') },
      },
      async onPrompt(sessionId: string, prompt: string) {
        calls.push(['prompt', sessionId, prompt])
      },
      getInjection(sessionId: string) {
        calls.push(['injection', sessionId])
        return 'coordinated hindsight context'
      },
    },
  }
  const adapter = createHindsightAdapter({
    harness: 'dsh',
    maxReflectTimeoutMs: 25_000,
    workspaceFor: () => workspace,
    ensureSeeded() {},
    async retainTranscript(retainedWorkspace, sessionId, events) {
      calls.push(['retain-transcript', retainedWorkspace, sessionId, events])
    },
    readEvents: () => [{ role: 'user', content: 'Remember SQLite' }],
    retainStamp: () => ({ tags: [], metadata: {} }),
    reflectQuery: query => query,
    operationId: value => value,
  })
  const baseMeta = {
    source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
    scope: '/workspace/project',
    attributes: { workspaceRoot: '/workspace/project', sessionId: 'session-1' },
  }

  assert.deepEqual(await adapter.retrieve({
    meta: { ...baseMeta, attributes: { ...baseMeta.attributes, point: 'agent/pre-step' } },
    data: {
      messages: [{ role: 'user', source: { kind: 'user' }, content: 'database choice' }],
    },
  }, {}), { text: 'coordinated hindsight context' })
  await adapter.update({
    meta: { ...baseMeta, attributes: { ...baseMeta.attributes, point: 'session/turn-end' } },
    data: {
      events: [{ type: 'turn/end' }],
      session: {
        events: [
          { type: 'user/message', seq: 1 },
          { type: 'assistant/message', seq: 2 },
          { type: 'turn/end', seq: 3 },
        ],
      },
    },
  }, {})
  assert.deepEqual(calls.slice(0, 2), [
    ['prompt', 'session-1', 'database choice'],
    ['injection', 'session-1'],
  ])
  assert.deepEqual(calls[2], [
    'retain-transcript',
    workspace,
    'session-1',
    [
      { type: 'user/message', seq: 1 },
      { type: 'assistant/message', seq: 2 },
      { type: 'turn/end', seq: 3 },
    ],
  ])
  assert.equal(calls.some(call => (call as unknown[])[0] === 'retain'), false)
})

test('fails closed when coordinated capture lacks the complete session transcript', async () => {
  const workspace = {
    root: '/workspace/project',
    core: {
      bankId: 'coding-agent::project',
      cfg: { reflectTimeoutMs: 10_000 },
      client: {
        async retain() { throw new Error('unexpected retain') },
        async reflect() { throw new Error('unexpected reflect') },
      },
    },
  }
  const adapter = createHindsightAdapter({
    harness: 'dsh',
    maxReflectTimeoutMs: 25_000,
    workspaceFor: () => workspace,
    ensureSeeded() {},
    async retainTranscript() { throw new Error('unexpected transcript retention') },
    readEvents: () => [],
    retainStamp: () => ({ tags: [], metadata: {} }),
    reflectQuery: query => query,
    operationId: value => value,
  })

  await assert.rejects(adapter.update({
    meta: {
      source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
      scope: '/workspace/project',
      attributes: {
        point: 'session/turn-end',
        workspaceRoot: '/workspace/project',
        sessionId: 'session-1',
      },
    },
    data: { events: [{ type: 'turn/end' }] },
  }, {}), /requires data\.session\.events/)
})
