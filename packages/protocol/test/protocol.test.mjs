import assert from 'node:assert/strict'
import test from 'node:test'

import {
  errorCodes,
  methods,
  protocolVersion,
} from '../lib/index.js'

test('publishes the versioned generic CRUD and reactive method names', () => {
  assert.equal(protocolVersion, 1)
  assert.deepEqual(methods, {
    handshake: 'patchouli.protocol.handshake@1',
    controlStatus: 'patchouli.control.status@1',
    controlShutdown: 'patchouli.control.shutdown@1',
    entityCreate: 'patchouli.entity.create@1',
    entityRead: 'patchouli.entity.read@1',
    entityUpdate: 'patchouli.entity.update@1',
    entityDelete: 'patchouli.entity.delete@1',
    changesSubscribe: 'patchouli.changes.subscribe@1',
    changesUnsubscribe: 'patchouli.changes.unsubscribe@1',
    changesEvent: 'patchouli.changes.event@1',
  })
})

test('keeps domain errors in the JSON-RPC server error range', () => {
  for (const code of Object.values(errorCodes)) {
    assert.ok(code >= -32099 && code <= -32000)
  }
})
