import assert from 'node:assert/strict'
import { Buffer } from 'node:buffer'
import { randomUUID } from 'node:crypto'
import { execFile, spawn } from 'node:child_process'
import { once } from 'node:events'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { promisify } from 'node:util'
import test from 'node:test'

import { Context } from '@deepseek-ai/cordis'

import * as crudTestPlugin from '../packages/crud-test-plugin/lib/index.js'
import * as patchouli from '../lib/index.js'
import * as storage from '../lib/storage.js'

const run = promisify(execFile)
const binary = resolve(
  'target',
  'debug',
  process.platform === 'win32' ? 'patchouli-db.exe' : 'patchouli-db',
)

function callMeta(operation) {
  return {
    source: { type: 'crud-test', id: 'database-loop' },
    scope: 'workspace-1',
    attributes: { operation },
  }
}

function valueOf(outcomes) {
  assert.equal(outcomes.length, 1)
  assert.equal(outcomes[0].pluginId, 'crud-test')
  assert.equal(outcomes[0].ok, true, outcomes[0].error)
  return outcomes[0].value
}

async function waitForDaemon(child, endpoint) {
  let stderr = ''
  child.stderr.setEncoding('utf8')
  await new Promise((resolveReady, rejectReady) => {
    const timeout = setTimeout(() => {
      rejectReady(new Error(`Patchouli daemon did not start at ${endpoint}: ${stderr}`))
    }, 10_000)
    const settle = (callback) => {
      clearTimeout(timeout)
      callback()
    }
    child.stderr.on('data', (chunk) => {
      stderr += chunk
      if (stderr.includes(`Patchouli daemon listening on ${endpoint}`)) {
        settle(resolveReady)
      }
    })
    child.once('error', error => settle(() => rejectReady(error)))
    child.once('exit', code => settle(() => rejectReady(
      new Error(`Patchouli daemon exited before startup with code ${code}: ${stderr}`),
    )))
  })
}

test('third-party plugin passes CRUD through the core service to SQLite', async (t) => {
  const root = await mkdtemp(join(tmpdir(), 'patchouli-crud-plugin-'))
  const endpoint = process.platform === 'win32'
    ? String.raw`\\.\pipe\patchouli-crud-${process.pid}-${randomUUID()}`
    : join(root, 'run', 'patchouli.sock')
  const config = join(root, 'config.json')
  const providers = join(root, 'providers.json')
  let daemon
  let ctx

  t.after(async () => {
    await ctx?.fiber.dispose()
    if (daemon?.exitCode === null) {
      try {
        await run(binary, ['stop', '--endpoint', endpoint])
        if (daemon.exitCode === null) await once(daemon, 'exit')
      } catch {
        if (daemon.exitCode === null) {
          daemon.kill()
          await once(daemon, 'exit')
        }
      }
    }
    await rm(root, { recursive: true })
  })

  await run(binary, ['init', '--root', root])
  daemon = spawn(binary, [
    'serve',
    '--endpoint', endpoint,
    '--artifacts', join(root, 'data', 'artifacts'),
    '--providers', providers,
    '--config', config,
  ], {
    cwd: dirname(config),
    stdio: ['ignore', 'ignore', 'pipe'],
  })
  await waitForDaemon(daemon, endpoint)

  ctx = new Context()
  await ctx.plugin(patchouli)
  await ctx.plugin(storage, {
    endpoint,
    command: binary,
    autoStart: false,
    startupTimeoutMs: 1_000,
  })
  await ctx.plugin(crudTestPlugin)

  const knowledge = JSON.parse(await readFile(
    resolve('packages/protocol/schemas/examples/knowledge@1.json'),
    'utf8',
  ))
  const artifactFixture = JSON.parse(await readFile(
    resolve('packages/protocol/schemas/examples/artifact-managed@1.json'),
    'utf8',
  ))
  const databaseMeta = {
    workspace_id: 'workspace-1',
    user_id: 'user-1',
    channel_id: 'channel-1',
  }
  const ref = { type: 'knowledge', id: 'crud-loop-1' }

  const artifactBytes = Uint8Array.from(
    { length: 600_000 },
    (_, index) => index % 251,
  )
  const uploadedArtifact = await ctx.patchouli.uploadArtifact({
    meta: databaseMeta,
    data: {
      id: 'artifact-e2e-1',
      media_type: 'application/octet-stream',
      name: 'artifact.bin',
      expected_byte_length: artifactBytes.byteLength,
      expected_digest: null,
      metadata: artifactFixture.metadata,
    },
  }, artifactBytes)
  assert.equal(uploadedArtifact.data.entity.ref.type, 'artifact')
  assert.equal(uploadedArtifact.data.entity.ref.id, 'artifact-e2e-1')
  assert.equal(uploadedArtifact.data.entity.value.placement.kind, 'managed')
  const downloadedArtifact = await ctx.patchouli.downloadArtifact(databaseMeta, 'artifact-e2e-1')
  assert.equal(downloadedArtifact.byteLength, artifactBytes.byteLength)
  assert.equal(Buffer.compare(Buffer.from(downloadedArtifact), Buffer.from(artifactBytes)), 0)

  const created = valueOf(await ctx.patchouliMemory.update({
    meta: callMeta('create'),
    data: {
      meta: databaseMeta,
      data: { type: ref.type, id: ref.id, value: knowledge },
    },
  }))
  assert.equal(created.data.entity.state, 'active')
  assert.deepEqual(created.data.entity.value, knowledge)

  const read = valueOf(await ctx.patchouliMemory.retrieve({
    meta: callMeta('read'),
    data: { meta: databaseMeta, data: { ref } },
  }))
  assert.equal(read.data.state, 'active')
  assert.deepEqual(read.data.variants[0].value, knowledge)

  const retrieved = valueOf(await ctx.patchouliMemory.retrieve({
    meta: callMeta('retrieve'),
    data: {
      meta: databaseMeta,
      data: { query: '代码审查', types: ['knowledge'], limit: 10 },
    },
  }))
  assert.equal(retrieved.data.hits.length, 1)
  assert.equal(retrieved.data.hits[0].variants[0].ref.id, ref.id)

  const updatedKnowledge = structuredClone(knowledge)
  updatedKnowledge.content.text = '用户偏好直接、简洁的代码审查意见'
  const updated = valueOf(await ctx.patchouliMemory.update({
    meta: callMeta('update'),
    data: {
      meta: { ...databaseMeta, base_versions: [created.data.entity.version] },
      data: { ref, value: updatedKnowledge },
    },
  }))
  assert.equal(updated.data.entity.state, 'active')
  assert.deepEqual(updated.data.entity.value, updatedKnowledge)

  const deleted = valueOf(await ctx.patchouliMemory.update({
    meta: callMeta('delete'),
    data: {
      meta: { ...databaseMeta, base_versions: [updated.data.entity.version] },
      data: { ref },
    },
  }))
  assert.equal(deleted.data.entity.state, 'deleted')

  const readDeleted = valueOf(await ctx.patchouliMemory.retrieve({
    meta: callMeta('read'),
    data: { meta: databaseMeta, data: { ref } },
  }))
  assert.equal(readDeleted.data.state, 'deleted')
  assert.equal(readDeleted.data.variants[0].state, 'deleted')
})
