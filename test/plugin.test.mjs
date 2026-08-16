import assert from 'node:assert/strict'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'
import {
  apply,
  inject,
  MemoryService,
  MemorySubscriptionError,
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
    meta: {
      source: { type: 'test', id: 'plugin-test' },
      scope: 'repo:memorax-agent/dsh-patchouli',
    },
    data: { messages: [{ role: 'user', content: 'remember this' }] },
  }
  const retrieveRequest = {
    meta: {
      source: { type: 'test', id: 'plugin-test' },
      scope: 'repo:memorax-agent/dsh-patchouli',
    },
    data: { query: 'what should be remembered?', limit: 5 },
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

test('routes calls through registration filters without exposing plugin selection to callers', async (t) => {
  const { fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const routes = []
  const invocations = []
  const plugin = id => ({
    id,
    async update(request) {
      invocations.push([id, 'update', request])
      return { status: 'applied' }
    },
    async retrieve(request) {
      invocations.push([id, 'retrieve', request])
      return { items: [{ content: id }] }
    },
  })
  const disposeSelected = memory.register(plugin('selected'), {
    filter(call) {
      routes.push(['selected', call])
      return call.operation === 'retrieve' && call.meta.source.type === 'agent-loop'
    },
  })
  const disposeSkipped = memory.register(plugin('skipped'), {
    filter(call) {
      routes.push(['skipped', call])
      return false
    },
  })
  const disposeBroken = memory.register(plugin('broken'), {
    filter(call) {
      routes.push(['broken', call])
      throw new Error('invalid route filter')
    },
  })
  t.after(() => {
    disposeBroken()
    disposeSkipped()
    disposeSelected()
  })

  const meta = {
    source: { type: 'agent-loop', id: 'test-consumer' },
    scope: 'repo:memorax-agent/dsh-patchouli',
    attributes: { trigger: 'pre-step' },
  }
  const retrieveRequest = { meta, data: { query: 'routing' } }
  assert.deepEqual(await memory.retrieve(retrieveRequest), [
    {
      pluginId: 'selected',
      ok: true,
      value: { items: [{ content: 'selected' }] },
    },
    {
      pluginId: 'broken',
      ok: false,
      error: 'invalid route filter',
    },
  ])
  assert.deepEqual(invocations, [['selected', 'retrieve', retrieveRequest]])

  const updateRequest = {
    meta,
    data: { messages: [{ role: 'user', content: 'do not route this operation' }] },
  }
  assert.deepEqual(await memory.update(updateRequest), [{
    pluginId: 'broken',
    ok: false,
    error: 'invalid route filter',
  }])
  assert.deepEqual(invocations, [['selected', 'retrieve', retrieveRequest]])
  assert.deepEqual(routes, [
    ['selected', { operation: 'retrieve', meta }],
    ['skipped', { operation: 'retrieve', meta }],
    ['broken', { operation: 'retrieve', meta }],
    ['selected', { operation: 'update', meta }],
    ['skipped', { operation: 'update', meta }],
    ['broken', { operation: 'update', meta }],
  ])
})

test('filters subscriptions and reports filter failures to the consumer', async (t) => {
  const { fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const selectedStarted = Promise.withResolvers()
  const selectedClosed = Promise.withResolvers()
  const basePlugin = id => ({
    id,
    async update() {
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
  })
  const disposeSelected = memory.register({
    ...basePlugin('selected'),
    async subscribe() {
      selectedStarted.resolve()
      return {
        cursor: 'selected-boundary',
        closed: selectedClosed.promise,
        async unsubscribe() {
          selectedClosed.resolve()
        },
      }
    },
  }, {
    filter: call => call.operation === 'subscribe',
  })
  const disposeSkipped = memory.register({
    ...basePlugin('skipped'),
    async subscribe() {
      throw new Error('filtered plugin must not start')
    },
  }, {
    filter: () => false,
  })
  const disposeBroken = memory.register({
    ...basePlugin('broken'),
    async subscribe() {
      throw new Error('broken filter plugin must not start')
    },
  }, {
    filter() {
      throw new Error('subscription filter failed')
    },
  })
  t.after(() => {
    disposeBroken()
    disposeSkipped()
    disposeSelected()
  })

  const failures = []
  const subscription = await memory.subscribe(
    {
      meta: {
        source: { type: 'consumer', id: 'reactive-index' },
        scope: 'test',
      },
    },
    async () => {},
    {
      cursorStore: {
        async load() {},
        async save() {},
        async delete() {},
      },
      onError(failure) {
        failures.push(failure)
      },
    },
  )

  await selectedStarted.promise
  assert.deepEqual(subscription.pluginIds, ['selected'])
  assert.deepEqual(failures.map(({ pluginId, error }) => ({
    pluginId,
    message: error.message,
  })), [{
    pluginId: 'broken',
    message: 'subscription filter failed',
  }])

  await subscription.unsubscribe()
  await subscription.closed
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

  const request = {
    meta: { source: { type: 'test', id: 'plugin-test' }, scope: 'test' },
    data: { query: 'memory' },
  }
  assert.equal((await memory.retrieve(request)).length, 1)

  await pluginFiber.dispose()
  assert.deepEqual(await memory.retrieve(request), [])
})

test('persists the subscription boundary and processes unique changes in order', async (t) => {
  const { fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const providerClosed = Promise.withResolvers()
  const boundarySaved = Promise.withResolvers()
  const firstStarted = Promise.withResolvers()
  const releaseFirst = Promise.withResolvers()
  const operations = []
  let emit
  let early
  let subscribeRequest
  let unsubscribeCount = 0
  const dispose = memory.register({
    id: 'streaming',
    async update() {
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
    async subscribe(request, handler) {
      subscribeRequest = request
      emit = handler
      early = handler({ cursor: 'cursor-1', memoryId: 'memory-1' })
      return {
        cursor: 'cursor-0',
        closed: providerClosed.promise,
        async unsubscribe() {
          unsubscribeCount += 1
          providerClosed.resolve()
        },
      }
    },
  })
  t.after(dispose)

  const cursorStore = {
    async load(pluginId) {
      operations.push(['load', pluginId])
      return 'cursor-before-subscribe'
    },
    async save(pluginId, cursor) {
      operations.push(['save', pluginId, cursor])
      if (cursor === 'cursor-0') boundarySaved.resolve()
    },
    async delete() {},
  }
  const changes = []
  const subscription = await memory.subscribe(
    {
      meta: {
        source: { type: 'consumer', id: 'test' },
        scope: '/workspace/patchouli',
      },
    },
    async (change) => {
      changes.push(['start', change])
      if (change.cursor === 'cursor-1') {
        firstStarted.resolve()
        await releaseFirst.promise
      }
      changes.push(['end', change.cursor])
    },
    { cursorStore },
  )

  await boundarySaved.promise
  await firstStarted.promise
  const duplicate = emit({ cursor: 'cursor-1', memoryId: 'duplicate' })
  const second = emit({ cursor: 'cursor-2', metadata: { source: 'test' } })

  assert.deepEqual(subscription.pluginIds, ['streaming'])
  assert.deepEqual(subscribeRequest, {
    meta: {
      source: { type: 'consumer', id: 'test' },
      scope: '/workspace/patchouli',
    },
    afterCursor: 'cursor-before-subscribe',
  })
  assert.deepEqual(operations, [
    ['load', 'streaming'],
    ['save', 'streaming', 'cursor-0'],
  ])
  assert.deepEqual(changes, [[
    'start',
    { pluginId: 'streaming', cursor: 'cursor-1', memoryId: 'memory-1' },
  ]])

  releaseFirst.resolve()
  await Promise.all([early, duplicate, second])
  assert.deepEqual(operations, [
    ['load', 'streaming'],
    ['save', 'streaming', 'cursor-0'],
    ['save', 'streaming', 'cursor-1'],
    ['save', 'streaming', 'cursor-2'],
  ])
  assert.deepEqual(changes, [
    ['start', { pluginId: 'streaming', cursor: 'cursor-1', memoryId: 'memory-1' }],
    ['end', 'cursor-1'],
    ['start', { pluginId: 'streaming', cursor: 'cursor-2', metadata: { source: 'test' } }],
    ['end', 'cursor-2'],
  ])

  await subscription.unsubscribe()
  await subscription.unsubscribe()
  await subscription.closed
  assert.equal(unsubscribeCount, 1)
})

test('retries only classified retryable subscription failures', async (t) => {
  const { fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const retryStarted = Promise.withResolvers()
  const retryBoundarySaved = Promise.withResolvers()
  const retryClosed = Promise.withResolvers()
  let retryAttempts = 0
  let fatalAttempts = 0
  const subscribeRequests = []
  const disposeRetry = memory.register({
    id: 'retryable',
    async update() {
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
    async subscribe(request, handler) {
      retryAttempts += 1
      subscribeRequests.push(request)
      if (retryAttempts === 1) {
        const disconnected = Promise.withResolvers()
        void handler({ cursor: 'durable-cursor' }).then(() => {
          disconnected.reject(new MemorySubscriptionError(
            'temporarily offline',
            { retryable: true },
          ))
        })
        return {
          cursor: 'first-boundary',
          closed: disconnected.promise,
          async unsubscribe() {},
        }
      }
      retryStarted.resolve()
      return {
        cursor: 'retry-boundary',
        closed: retryClosed.promise,
        async unsubscribe() {
          retryClosed.resolve()
        },
      }
    },
  })
  const disposeFatal = memory.register({
    id: 'fatal',
    async update() {
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
    async subscribe() {
      fatalAttempts += 1
      throw new Error('invalid subscription')
    },
  })
  t.after(() => {
    disposeFatal()
    disposeRetry()
  })

  const requests = []
  const failures = []
  const subscription = await memory.subscribe(
    { meta: { source: { type: 'test', id: 'plugin-test' }, scope: 'test' } },
    async () => {},
    {
      cursorStore: {
        async load(pluginId) {
          requests.push(['load', pluginId])
          return 'saved-cursor'
        },
        async save(pluginId, cursor) {
          requests.push(['save', pluginId, cursor])
          if (pluginId === 'retryable' && cursor === 'retry-boundary') {
            retryBoundarySaved.resolve()
          }
        },
        async delete() {},
      },
      onError(failure) {
        failures.push(failure)
      },
    },
  )

  await retryStarted.promise
  await retryBoundarySaved.promise
  assert.equal(retryAttempts, 2)
  assert.equal(fatalAttempts, 1)
  assert.deepEqual(subscribeRequests.map(request => request.afterCursor), [
    'saved-cursor',
    'durable-cursor',
  ])
  assert.deepEqual(failures.map(({ pluginId, error }) => ({
    pluginId,
    message: error.message,
    retryable: error.retryable,
    resetRequired: error.resetRequired,
  })).sort((left, right) => left.pluginId.localeCompare(right.pluginId)), [
    {
      pluginId: 'fatal',
      message: 'invalid subscription',
      retryable: false,
      resetRequired: false,
    },
    {
      pluginId: 'retryable',
      message: 'temporarily offline',
      retryable: true,
      resetRequired: false,
    },
  ])
  assert.deepEqual(requests.filter(([operation]) => operation === 'save'), [
    ['save', 'retryable', 'first-boundary'],
    ['save', 'retryable', 'durable-cursor'],
    ['save', 'retryable', 'retry-boundary'],
  ])

  await subscription.unsubscribe()
  await subscription.closed
})

test('observes fast disconnects and resets retry only after a durable change', async (t) => {
  const { fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const originalSetTimeout = globalThis.setTimeout
  const originalRandom = Math.random
  const retryDelays = []
  globalThis.setTimeout = (callback, delay, ...args) => {
    retryDelays.push(delay)
    return originalSetTimeout(callback, 0, ...args)
  }
  Math.random = () => 0
  t.after(() => {
    globalThis.setTimeout = originalSetTimeout
    Math.random = originalRandom
  })

  const stable = Promise.withResolvers()
  const stableClosed = Promise.withResolvers()
  let attempts = 0
  const dispose = memory.register({
    id: 'flapping',
    async update() {
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
    async subscribe(_request, handler) {
      attempts += 1
      if (attempts <= 2) {
        return {
          cursor: `boundary-${attempts}`,
          closed: Promise.reject(new MemorySubscriptionError(
            'immediate disconnect',
            { retryable: true },
          )),
          async unsubscribe() {},
        }
      }
      if (attempts === 3) {
        const disconnected = Promise.withResolvers()
        void handler({ cursor: 'durable-change' }).then(() => {
          disconnected.reject(new MemorySubscriptionError(
            'disconnect after progress',
            { retryable: true },
          ))
        })
        return {
          cursor: 'progress-boundary',
          closed: disconnected.promise,
          async unsubscribe() {},
        }
      }

      stable.resolve()
      return {
        cursor: 'stable-boundary',
        closed: stableClosed.promise,
        async unsubscribe() {
          stableClosed.resolve()
        },
      }
    },
  })
  t.after(dispose)

  const subscription = await memory.subscribe(
    { meta: { source: { type: 'test', id: 'plugin-test' }, scope: 'test' } },
    async () => {},
    {
      cursorStore: {
        async load() {},
        async save(_pluginId, cursor) {
          if (cursor === 'boundary-1') {
            await new Promise(resolve => originalSetTimeout(resolve, 20))
          }
        },
        async delete() {},
      },
    },
  )

  await stable.promise
  assert.equal(attempts, 4)
  assert.deepEqual(retryDelays, [250, 500, 250])

  globalThis.setTimeout = originalSetTimeout
  Math.random = originalRandom
  await subscription.unsubscribe()
  await subscription.closed
})

test('disposes and drains a subscription with its consuming Cordis fiber', async (t) => {
  const { ctx, fiber, memory } = await mountPatchouli()
  t.after(() => fiber.dispose())

  const providerStarted = Promise.withResolvers()
  const providerClosed = Promise.withResolvers()
  let unsubscribeCount = 0
  const dispose = memory.register({
    id: 'owned-stream',
    async update() {
      return { status: 'applied' }
    },
    async retrieve() {
      return { items: [] }
    },
    async subscribe() {
      providerStarted.resolve()
      return {
        cursor: 'boundary',
        closed: providerClosed.promise,
        async unsubscribe() {
          unsubscribeCount += 1
          providerClosed.resolve()
        },
      }
    },
  })
  t.after(dispose)

  let subscription
  const consumerFiber = await ctx.plugin({
    name: 'memory-change-consumer',
    inject: ['patchouliMemory'],
    async apply(pluginCtx) {
      subscription = await pluginCtx.patchouliMemory.subscribe(
        { meta: { source: { type: 'test', id: 'plugin-test' }, scope: 'test' } },
        async () => {},
        {
          cursorStore: {
            async load() {},
            async save() {},
            async delete() {},
          },
        },
      )
    },
  })

  await providerStarted.promise
  await consumerFiber.dispose()
  await subscription.closed
  assert.equal(unsubscribeCount, 1)
})
