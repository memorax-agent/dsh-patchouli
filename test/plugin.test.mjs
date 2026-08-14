import assert from 'node:assert/strict'
import test from 'node:test'

import { apply, inject, name } from '../lib/index.js'

test('exports a loadable Cordis plugin', () => {
  assert.equal(name, 'dsh-patchouli')
  assert.deepEqual(inject, [])
  assert.equal(apply({}), undefined)
})
