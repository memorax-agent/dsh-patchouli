import assert from 'node:assert/strict'
import test from 'node:test'

import * as artifactIngestor from '../packages/artifact-ingestor/lib/index.js'
import * as crudTestPlugin from '../packages/crud-test-plugin/lib/index.js'
import * as sessionIndexer from '../packages/session-indexer/lib/index.js'
import * as workspaceIndexer from '../packages/workspace-indexer/lib/index.js'

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
