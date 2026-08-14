import { spawn, spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, isAbsolute, join, resolve } from 'node:path'
import process from 'node:process'

const source = resolve(process.argv[2] ?? '')
if (!process.argv[2] || !existsSync(source)) throw new Error('release daemon binary is missing')

const deployRoot = resolve(process.env.PATCHOULI_DEPLOY_ROOT || join(homedir(), '.patchouli'))
if (!isAbsolute(deployRoot) || deployRoot === resolve(homedir()) || deployRoot === resolve('/')) {
  throw new Error(`unsafe deployment root: ${deployRoot}`)
}

const binaryName = process.platform === 'win32' ? 'patchouli.exe' : 'patchouli'
const installDir = join(deployRoot, 'bin')
const target = join(installDir, binaryName)
const backup = join(installDir, `${binaryName}.previous`)
const endpoint = process.env.PATCHOULI_ENDPOINT || (process.platform === 'win32'
  ? String.raw`\\.\pipe\patchouli`
  : join(deployRoot, 'run', 'patchouli.sock'))
const database = process.env.PATCHOULI_DATABASE || join(deployRoot, 'data', 'patchouli.db')
const config = resolve(process.env.PATCHOULI_CONFIG || 'config/patchouli.default.json')

for (const path of [installDir, join(deployRoot, 'run'), join(deployRoot, 'data')]) {
  mkdirSync(path, { recursive: true, mode: 0o700 })
}

run(source, ['config', 'check', config], 'new daemon rejected its backend configuration')
const hadPrevious = existsSync(target)
if (hadPrevious) copyFileSync(target, backup)

const status = spawnSync(source, ['status', '--endpoint', endpoint], { encoding: 'utf8' })
if (status.status === 0) run(source, ['stop', '--endpoint', endpoint], 'failed to stop current daemon')

copyFileSync(source, target)
if (process.platform !== 'win32') spawnSync('chmod', ['700', target])
start(target)

if (!waitUntilReady(target, 10_000)) {
  spawnSync(target, ['stop', '--endpoint', endpoint], { encoding: 'utf8' })
  if (hadPrevious) {
    copyFileSync(backup, target)
    start(target)
    if (!waitUntilReady(target, 10_000)) {
      throw new Error('new daemon failed health check and the previous daemon could not be restored')
    }
  }
  throw new Error('new daemon failed health check; previous daemon restored')
}

console.log(`deployed ${basename(target)} to ${installDir}`)

function start(command) {
  const child = spawn(command, [
    'serve',
    '--endpoint', endpoint,
    '--database', database,
    '--config', config,
  ], {
    detached: true,
    stdio: 'ignore',
    env: { ...process.env, RUNNER_TRACKING_ID: '' },
  })
  child.unref()
}

function waitUntilReady(command, timeoutMs) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const result = spawnSync(command, ['status', '--endpoint', endpoint], { encoding: 'utf8' })
    if (result.status === 0) return true
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100)
  }
  return false
}

function run(command, args, message) {
  const result = spawnSync(command, args, { encoding: 'utf8', stdio: 'inherit' })
  if (result.status !== 0) throw new Error(message)
}
