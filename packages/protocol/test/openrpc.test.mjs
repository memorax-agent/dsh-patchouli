import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import jsonSchemaMetaPackage from '@json-schema-tools/meta-schema'
import metaSchemaPackage from '@open-rpc/meta-schema'
import Ajv from 'ajv'

import { errorCodes, methods } from '../lib/index.js'

const document = JSON.parse(
  readFileSync(new URL('../openrpc.json', import.meta.url), 'utf8'),
)
const metaSchema = metaSchemaPackage.default
const jsonSchemaMeta = jsonSchemaMetaPackage.default

test('openrpc.json conforms to the OpenRPC meta-schema', () => {
  const ajv = new Ajv({ strict: false, validateSchema: false })
  ajv.addFormat('uri', true)
  ajv.addFormat('uri-reference', true)
  ajv.addFormat('regex', true)
  ajv.addMetaSchema(jsonSchemaMeta)
  const normalizedMetaSchema = structuredClone(metaSchema)
  normalizedMetaSchema.definitions.JSONSchema.$ref = 'https://meta.json-schema.tools/'
  const valid = ajv.validate(normalizedMetaSchema, document)

  assert.equal(valid, true, ajv.errorsText(ajv.errors, { separator: '\n' }))
})

test('OpenRPC, TypeScript methods, and error codes stay in sync', () => {
  const documentedMethods = document.methods.map(({ name }) => name).sort()
  const exportedMethods = Object.values(methods).sort()
  assert.deepEqual(documentedMethods, exportedMethods)

  const documentedErrors = Object.fromEntries(
    Object.entries(document.components.errors).map(([name, { code }]) => [name, code]),
  )
  assert.deepEqual(documentedErrors, {
    Cancelled: errorCodes.cancelled,
    CursorExpired: errorCodes.cursorExpired,
    DeadlineExceeded: errorCodes.deadlineExceeded,
    Forbidden: errorCodes.forbidden,
    IdempotencyConflict: errorCodes.idempotencyConflict,
    InvalidRequest: errorCodes.invalidRequest,
    NotFound: errorCodes.notFound,
    Overloaded: errorCodes.overloaded,
    Unauthenticated: errorCodes.unauthenticated,
    UnsupportedCapability: errorCodes.unsupportedCapability,
    VersionConflict: errorCodes.versionConflict,
    WorkUnitExpired: errorCodes.workUnitExpired,
  })
})

test('all local OpenRPC references resolve', () => {
  const references = []

  const visit = (value) => {
    if (Array.isArray(value)) {
      value.forEach(visit)
      return
    }
    if (value === null || typeof value !== 'object') return
    if (typeof value.$ref === 'string' && value.$ref.startsWith('#/')) {
      references.push(value.$ref)
    }
    Object.values(value).forEach(visit)
  }

  visit(document)

  for (const reference of references) {
    const target = reference
      .slice(2)
      .split('/')
      .reduce((value, segment) => value?.[segment], document)
    assert.notEqual(target, undefined, `unresolved reference: ${reference}`)
  }
})

test('business methods expose only meta and data parameters', () => {
  const resolve = (value) => {
    if (!value.$ref) return value
    return value.$ref
      .slice(2)
      .split('/')
      .reduce((target, segment) => target[segment], document)
  }

  for (const method of document.methods) {
    if (method.name === methods.handshake) continue
    assert.deepEqual(
      method.params.map((parameter) => resolve(parameter).name),
      ['meta', 'data'],
      method.name,
    )
  }
})

test('handshake capabilities use one list schema in both directions', () => {
  const handshake = document.methods.find(({ name }) => name === methods.handshake)
  const requestCapabilities = handshake.params.find(({ name }) => name === 'capabilities')
  const resultCapabilities = document.components.schemas.HandshakeResult.properties.capabilities

  assert.equal(requestCapabilities.schema.$ref, '#/components/schemas/CapabilityList')
  assert.equal(resultCapabilities.$ref, '#/components/schemas/CapabilityList')
})
