import assert from 'node:assert/strict'
import test from 'node:test'

import {
  errorCodes,
  factEntityTypes,
  factSchemaUris,
  methods,
  protocolVersion,
} from '../lib/index.js'

test('publishes the versioned generic CRUD and reactive method names', () => {
  assert.equal(protocolVersion, 1)
  assert.deepEqual(methods, {
    handshake: 'patchouli.protocol.handshake@1',
    controlStatus: 'patchouli.control.status@1',
    controlCheckpoint: 'patchouli.control.checkpoint@1',
    controlShutdown: 'patchouli.control.shutdown@1',
    entityCreate: 'patchouli.entity.create@1',
    entityRead: 'patchouli.entity.read@1',
    entityRetrieve: 'patchouli.entity.retrieve@1',
    entityUpdate: 'patchouli.entity.update@1',
    entityDelete: 'patchouli.entity.delete@1',
    changesSubscribe: 'patchouli.changes.subscribe@1',
    changesUnsubscribe: 'patchouli.changes.unsubscribe@1',
    changesEvent: 'patchouli.changes.event@1',
  })
})

test('publishes the typed fact identities without adding RPC methods', () => {
  assert.deepEqual(factEntityTypes, {
    knowledge: 'knowledge',
    knowledgeRelation: 'knowledge_relation',
  })
  assert.deepEqual(factSchemaUris, {
    common: 'urn:patchouli:schema:fact-common:1',
    knowledge: 'urn:patchouli:schema:knowledge:1',
    knowledgeRelation: 'urn:patchouli:schema:knowledge-relation:1',
  })
})

test('keeps domain errors in the JSON-RPC server error range', () => {
  assert.equal(errorCodes.invalidRequest, -32602)
  for (const [name, code] of Object.entries(errorCodes)) {
    if (name === 'invalidRequest') continue
    assert.ok(code >= -32099 && code <= -32000)
  }
})
