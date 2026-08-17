import assert from 'node:assert/strict'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'
import type { Domain, KvTable } from '@deepseek-ai/dsh-storage-domain'
import CursorStoreService, { memoryCursorDomainSpec } from '../lib/cursor-store.js'

interface CursorRecord {
  cursor: string
}

async function mountCursorStore() {
  const records = new Map<string, CursorRecord>()
  let closeCalls = 0
  let openedSpec: typeof memoryCursorDomainSpec | undefined
  const table: KvTable<string, CursorRecord> = {
    get: key => records.get(key),
    entries: () => new Map(records).entries(),
    keys: () => new Map(records).keys(),
    get size() { return records.size },
    async put(key, value) {
      records.set(key, value)
    },
    async delete(key) {
      return records.delete(key)
    },
    async update(key, transform) {
      const current = records.get(key)
      if (current === undefined) throw new Error(`missing key: ${key}`)
      const value = transform(current)
      records.set(key, value)
      return value
    },
  }
  const domain = {
    name: 'patchouli_memory',
    table(name: string) {
      assert.equal(name, 'cursors')
      return table
    },
    async close() {
      closeCalls += 1
    },
  }
  const ctx = new Context()
  const storageDomain = {
    async open(spec: typeof memoryCursorDomainSpec) {
      openedSpec = spec
      return domain as unknown as Domain<typeof memoryCursorDomainSpec>
    },
  } as Context['storageDomain']
  ctx.provide('storageDomain', storageDomain)
  const fiber = await ctx.plugin(CursorStoreService)
  return {
    ctx,
    fiber,
    records,
    openedSpec: () => {
      assert.ok(openedSpec)
      return openedSpec
    },
    closeCalls: () => closeCalls,
  }
}

test('persists isolated cursors with the storage domain service', async () => {
  const harness = await mountCursorStore()
  const { ctx, fiber, records } = harness

  assert.equal(harness.openedSpec().name, 'patchouli_memory')
  assert.deepEqual(Object.keys(harness.openedSpec().tables), ['cursors'])

  const main = ctx.patchouliCursors.bind({
    consumerId: 'agent-loop',
    subscriptionKey: 'memory-changes',
    scope: 'repo:a',
  })
  const otherScope = ctx.patchouliCursors.bind({
    consumerId: 'agent-loop',
    subscriptionKey: 'memory-changes',
    scope: 'repo:b',
  })
  const otherSubscription = ctx.patchouliCursors.bind({
    consumerId: 'agent-loop',
    subscriptionKey: 'other-changes',
    scope: 'repo:a',
  })
  const rawScope = ctx.patchouliCursors.bind({
    consumerId: 'agent-loop',
    subscriptionKey: 'memory-changes',
    scope: ' repo:a ',
  })

  await main.save('memorax', 'cursor-1')
  await main.save('other-plugin', 'cursor-2')
  await otherScope.save('memorax', 'cursor-3')
  await otherSubscription.save('memorax', 'cursor-4')
  await rawScope.save('memorax', 'cursor-5')

  assert.equal(await main.load('memorax'), 'cursor-1')
  assert.equal(await main.load('missing'), undefined)
  assert.equal(await otherScope.load('memorax'), 'cursor-3')
  assert.equal(await otherSubscription.load('memorax'), 'cursor-4')
  assert.deepEqual([...records], [
    [JSON.stringify(['agent-loop', 'memory-changes', 'repo:a', 'memorax']), { cursor: 'cursor-1' }],
    [JSON.stringify(['agent-loop', 'memory-changes', 'repo:a', 'other-plugin']), { cursor: 'cursor-2' }],
    [JSON.stringify(['agent-loop', 'memory-changes', 'repo:b', 'memorax']), { cursor: 'cursor-3' }],
    [JSON.stringify(['agent-loop', 'other-changes', 'repo:a', 'memorax']), { cursor: 'cursor-4' }],
    [JSON.stringify(['agent-loop', 'memory-changes', ' repo:a ', 'memorax']), { cursor: 'cursor-5' }],
  ])

  assert.equal(await main.delete('memorax'), undefined)
  assert.equal(await main.load('memorax'), undefined)

  await fiber.dispose()
  assert.equal(harness.closeCalls(), 1)
})

test('rejects blank cursor binding identities', async (t) => {
  const { ctx, fiber } = await mountCursorStore()
  t.after(() => fiber.dispose())

  for (const field of ['consumerId', 'subscriptionKey', 'scope']) {
    const binding = {
      consumerId: 'agent-loop',
      subscriptionKey: 'memory-changes',
      scope: 'repo:a',
      [field]: ' \t ',
    }
    assert.throws(
      () => ctx.patchouliCursors.bind(binding),
      new RegExp(`memory cursor ${field} must be a non-empty string`),
    )
  }
})
