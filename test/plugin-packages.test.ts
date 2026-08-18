import assert from 'node:assert/strict'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'
import * as artifactIngestor from '../packages/artifact-ingestor/lib/index.js'
import * as crudTestPlugin from '../packages/crud-test-plugin/lib/index.js'
import * as sessionIndexer from '../packages/session-indexer/lib/index.js'
import * as workspaceIndexer from '../packages/workspace-indexer/lib/index.js'
import * as patchouli from '../lib/index.js'

test('CRUD test plugin declares an isolated third-party boundary', () => {
  assert.equal(crudTestPlugin.name, 'dsh-patchouli-crud-test-plugin')
  assert.deepEqual(crudTestPlugin.inject, ['patchouli', 'patchouliStorage'])
})

test('artifact ingestor declares the DSH byte-source boundary', () => {
  assert.equal(artifactIngestor.name, 'dsh-patchouli-artifact-ingestor')
  assert.deepEqual(artifactIngestor.inject, [
    'patchouli',
    'patchouliStorage',
    'attachments',
    'fs',
  ])
})

test('artifact ingestor scans only the current durable turn for images', async (t) => {
  const ctx = new Context()
  const core = await ctx.plugin(patchouli)
  t.after(() => core.dispose())

  const image = {
    type: 'image',
    attachment: {
      attachmentId: 'attachment-1',
      mediaType: 'image/png',
      bytes: 3,
      width: 1,
      height: 1,
      name: 'fixture.png',
    },
  }
  let uploads = 0
  const disposeAttachments = ctx.provide('attachments', {
    async readImage(ref: typeof image.attachment) {
      return { ref, data: new Uint8Array([1, 2, 3]) }
    },
  })
  const disposeStorage = ctx.provide('patchouliStorage', {
    async uploadArtifact() {
      uploads += 1
      return {
        data: {
          entity: {
            state: 'active',
            ref: { type: 'artifact', id: `artifact-${uploads}` },
            version: `version-${uploads}`,
          },
        },
      }
    },
  })
  const disposeFs = ctx.provide('fs', {})
  t.after(async () => {
    await disposeFs()
    await disposeStorage()
    await disposeAttachments()
  })
  const ingestor = await ctx.plugin(artifactIngestor, {})
  t.after(() => ingestor.dispose())

  const meta = {
    source: { type: 'agent-loop', id: 'fixture' },
    scope: '/workspace',
    attributes: { point: 'session/turn-end', sessionId: 'session-1' },
  }
  const historic = { type: 'user/message', data: { content: [image] } }
  await ctx.patchouli.update({
    meta,
    data: { session: { events: [historic] }, events: [] },
  })
  assert.equal(uploads, 0)

  await ctx.patchouli.update({
    meta,
    data: { session: { events: [historic] }, events: [historic] },
  })
  assert.equal(uploads, 1)

  await ctx.patchouli.update({
    meta,
    data: { session: { events: [historic] }, events: [] },
  })
  assert.equal(uploads, 1)
})

test('session indexer declares its package boundary', () => {
  assert.equal(sessionIndexer.name, 'dsh-patchouli-session-indexer')
  assert.deepEqual(sessionIndexer.inject, ['patchouli', 'sessionQuery'])
  assert.equal(sessionIndexer.apply(), undefined)
})

test('workspace indexer declares its package boundary', () => {
  assert.equal(workspaceIndexer.name, 'dsh-patchouli-workspace-indexer')
  assert.deepEqual(workspaceIndexer.inject, ['patchouli', 'workspaceRegistry', 'fs'])
  assert.equal(workspaceIndexer.apply(), undefined)
})
