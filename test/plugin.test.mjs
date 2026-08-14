import assert from 'node:assert/strict'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'
import {
  apply,
  inject,
  MemoryService,
  name,
} from '../lib/index.js'

async function mountPatchouli() {
  const ctx = new Context()
  const fiber = await ctx.plugin({ name, inject, apply })
  return { ctx, fiber, memory: ctx.patchouliMemory }
}

test('mounts the common memory service', async (t) => {
  const { ctx, fiber } = await mountPatchouli()
  t.after(() => fiber.dispose())

  assert.equal(name, 'dsh-patchouli')
  assert.deepEqual(inject, [])
  assert.ok(ctx.patchouliMemory instanceof MemoryService)
})

test('routes update and retrieve to registered plugins and aggregates outcomes', async (t) => {
  const { fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const seen = []
  const disposeFirst = memory.register({
    id: 'first',
    async update(request, context) {
      seen.push(['first', 'update', request, context.signal])
      return { status: 'accepted', receipt: 'receipt-1' }
    },
    async retrieve(request, context) {
      seen.push(['first', 'retrieve', request, context.signal])
      return { items: [{ id: 'm1', content: 'first memory', score: 0.9 }] }
    },
  })
  const disposeSecond = memory.register({
    id: 'second',
    async update(request, context) {
      seen.push(['second', 'update', request, context.signal])
      throw new Error('write unavailable')
    },
    async retrieve(request, context) {
      seen.push(['second', 'retrieve', request, context.signal])
      return { items: [{ content: 'second memory' }] }
    },
  })
  t.after(() => {
    disposeSecond()
    disposeFirst()
  })

  const controller = new AbortController()
  const updateRequest = {
    scope: 'repo:memorax-agent/dsh-patchouli',
    messages: [{ role: 'user', content: 'remember this' }],
  }
  const retrieveRequest = {
    scope: 'repo:memorax-agent/dsh-patchouli',
    query: 'what should be remembered?',
    limit: 5,
  }

  assert.deepEqual(await memory.update(updateRequest, controller.signal), [
    {
      pluginId: 'first',
      ok: true,
      value: { status: 'accepted', receipt: 'receipt-1' },
    },
    {
      pluginId: 'second',
      ok: false,
      error: 'write unavailable',
    },
  ])
  assert.deepEqual(await memory.retrieve(retrieveRequest, controller.signal), [
    {
      pluginId: 'first',
      ok: true,
      value: { items: [{ id: 'm1', content: 'first memory', score: 0.9 }] },
    },
    {
      pluginId: 'second',
      ok: true,
      value: { items: [{ content: 'second memory' }] },
    },
  ])
  assert.deepEqual(seen, [
    ['first', 'update', updateRequest, controller.signal],
    ['second', 'update', updateRequest, controller.signal],
    ['first', 'retrieve', retrieveRequest, controller.signal],
    ['second', 'retrieve', retrieveRequest, controller.signal],
  ])
})

test('removes a memory plugin with its registering Cordis fiber', async (t) => {
  const { ctx, fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const pluginFiber = await ctx.plugin({
    name: 'temporary-memory-plugin',
    inject: ['patchouliMemory'],
    apply(pluginCtx) {
      pluginCtx.patchouliMemory.register({
        id: 'temporary',
        async update() {
          return { status: 'applied' }
        },
        async retrieve() {
          return { items: [] }
        },
      })
    },
  })

  const request = { scope: 'test', query: 'memory' }
  assert.equal((await memory.retrieve(request)).length, 1)

  await pluginFiber.dispose()
  assert.deepEqual(await memory.retrieve(request), [])
})
