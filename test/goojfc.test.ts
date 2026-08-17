import assert from 'node:assert/strict'
import test from 'node:test'

import { Context, type Fiber } from '@deepseek-ai/cordis'

import {
  apply,
  bridgeServices,
  inject,
  name,
  provide,
} from '../lib/goojfc/index.js'
import * as patchouli from '../lib/index.js'

test('declares the temporary compatibility plugin boundary', () => {
  assert.equal(name, 'dsh-patchouli-goojfc')
  assert.deepEqual(inject, ['patchouli'])
  assert.equal(provide, 'patchouliGoojfc')
  assert.deepEqual(bridgeServices, [
    'goojfcOpenViking',
    'goojfcHindsight',
    'goojfcMemos',
    'goojfcMneme',
    'goojfcMnemon',
    'goojfcMemoryGate',
    'goojfcLingshu',
    'goojfcGraphMemory',
    'goojfcEngramory',
    'goojfcMemoryEvolve',
  ])
  assert.equal(typeof apply, 'function')
})

test('registers an available native bridge with the memory frontend', async (t) => {
  const ctx = new Context()
  const fibers: Fiber[] = []
  fibers.push(await ctx.plugin(patchouli))
  fibers.push(await ctx.plugin({ apply, inject, name }))
  t.after(async () => {
    for (const fiber of fibers.reverse()) await fiber.dispose()
  })

  const calls: unknown[] = []
  assert.equal(typeof ctx.patchouliGoojfc.createOpenVikingAdapter, 'function')
  assert.equal(typeof ctx.patchouliGoojfc.createMemoryEvolveAdapter, 'function')
  const dispose = ctx.provide('goojfcOpenViking', {
    id: 'openviking',
    filter(call: { meta: { source: { type: string } } }) {
      return call.meta.source.type === 'test'
    },
    async update(request: unknown) {
      calls.push(['update', request])
      return { stored: true }
    },
    async retrieve(request: unknown) {
      calls.push(['retrieve', request])
      return { items: ['native result'] }
    },
  })
  t.after(dispose)
  await Promise.resolve()

  const request = {
    meta: {
      source: { type: 'test', id: 'goojfc' },
      scope: 'fixture',
    },
    data: { query: 'native' },
  }
  assert.deepEqual(await ctx.patchouli.retrieve(request), [{
    pluginId: 'openviking',
    ok: true,
    value: { items: ['native result'] },
  }])
  assert.deepEqual(calls, [['retrieve', request]])

  assert.deepEqual(await ctx.patchouli.update({
    meta: {
      ...request.meta,
      attributes: { point: 'session/turn-end' },
    },
    data: { events: [{ type: 'user/message' }] },
  }), [{
    pluginId: 'openviking',
    ok: true,
    value: { stored: true },
  }])
  assert.deepEqual(calls, [
    ['retrieve', request],
    ['update', {
      meta: {
        ...request.meta,
        attributes: { point: 'session/turn-end' },
      },
      data: { events: [{ type: 'user/message' }] },
    }],
  ])

  assert.deepEqual(await ctx.patchouli.retrieve({
    ...request,
    meta: { ...request.meta, source: { type: 'other', id: 'goojfc' } },
  }), [])
  assert.equal(calls.length, 2)
})

test('fans one coordinated call out to all matching passive adapters', async (t) => {
  const ctx = new Context()
  const fibers: Fiber[] = []
  fibers.push(await ctx.plugin(patchouli))
  fibers.push(await ctx.plugin({ apply, inject, name }))
  t.after(async () => {
    for (const fiber of fibers.reverse()) await fiber.dispose()
  })

  for (const serviceName of bridgeServices) {
    const pluginId = serviceName.replace(/^goojfc/, '').toLowerCase()
    const dispose = ctx.provide(serviceName, {
      id: pluginId,
      async update() { return { point: 'session/turn-end' } },
      async retrieve() { return { point: 'agent/pre-step' } },
    })
    t.after(dispose)
  }
  await Promise.resolve()

  const meta = {
    source: { type: 'agent-loop', id: 'dsh-patchouli-agent-loop' },
    scope: '/workspace/project',
    attributes: { point: 'agent/pre-step' },
  }
  const recalled = await ctx.patchouli.retrieve({ meta, data: { query: 'database' } })
  assert.equal(recalled.length, 10)
  assert.ok(recalled.every(outcome => outcome.ok))

  const stored = await ctx.patchouli.update({
    meta: { ...meta, attributes: { point: 'session/turn-end' } },
    data: { events: [] },
  })
  assert.deepEqual(stored.map(outcome => outcome.pluginId), [
    'openviking',
    'hindsight',
    'memos',
    'mneme',
    'memorygate',
    'lingshu',
  ])
})
