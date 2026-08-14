import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import Ajv2020 from 'ajv/dist/2020.js'

const schemaRoot = new URL('../schemas/', import.meta.url)

async function readJson(relativePath) {
  return JSON.parse(await readFile(new URL(relativePath, schemaRoot), 'utf8'))
}

test('knowledge and knowledge relation examples conform to their fact schemas', async () => {
  const [commonSchema, knowledgeSchema, relationSchema, knowledge, relation] = await Promise.all([
    readJson('fact-common@1.schema.json'),
    readJson('knowledge@1.schema.json'),
    readJson('knowledge-relation@1.schema.json'),
    readJson('examples/knowledge@1.json'),
    readJson('examples/knowledge-relation@1.json'),
  ])
  const ajv = new Ajv2020({ allErrors: true, strict: false })
  ajv.addFormat('date-time', {
    type: 'string',
    validate: value => /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(value),
  })
  ajv.addSchema(commonSchema)
  const validateKnowledge = ajv.compile(knowledgeSchema)
  const validateRelation = ajv.compile(relationSchema)

  assert.equal(validateKnowledge(knowledge), true, JSON.stringify(validateKnowledge.errors))
  assert.equal(validateRelation(relation), true, JSON.stringify(validateRelation.errors))
  assert.equal(relation.from.length, 2)
  assert.equal(relation.to.length, 2)

  const invalidRelation = structuredClone(relation)
  invalidRelation.from[0].type = 'artifact'
  assert.equal(validateRelation(invalidRelation), false)

  const emptyRelation = structuredClone(relation)
  emptyRelation.to = []
  assert.equal(validateRelation(emptyRelation), false)
})
