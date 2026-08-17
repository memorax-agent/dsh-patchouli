import { spawn, spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { basename, isAbsolute, join, resolve } from 'node:path'
import process from 'node:process'

const source = resolve(process.argv[2] ?? '')
if (!process.argv[2] || !existsSync(source)) throw new Error('release daemon binary is missing')

const deployRoot = resolve(process.env.PATCHOULI_DEPLOY_ROOT || join(homedir(), '.patchouli'))
if (!isAbsolute(deployRoot) || deployRoot === resolve(homedir()) || deployRoot === resolve('/')) {
  throw new Error(`unsafe deployment root: ${deployRoot}`)
}

const binaryName = process.platform === 'win32' ? 'patchouli-db.exe' : 'patchouli-db'
const installDir = join(deployRoot, 'bin')
const target = join(installDir, binaryName)
const backup = join(installDir, `${binaryName}.previous`)
const endpoint = process.env.PATCHOULI_ENDPOINT || (process.platform === 'win32'
  ? String.raw`\\.\pipe\patchouli`
  : join(deployRoot, 'run', 'patchouli.sock'))
const config = resolve(process.env.PATCHOULI_CONFIG || 'config/patchouli.default.json')
const providers = resolve(process.env.PATCHOULI_PROVIDERS || join(deployRoot, 'providers.json'))
const artifacts = resolve(process.env.PATCHOULI_ARTIFACTS || join(deployRoot, 'data', 'artifacts'))

for (const path of [installDir, join(deployRoot, 'run'), join(deployRoot, 'data')]) {
  mkdirSync(path, { recursive: true, mode: 0o700 })
}

if (!process.env.PATCHOULI_PROVIDERS && !existsSync(providers)) {
  writeFileSync(providers, `${JSON.stringify({
    $schema: 'https://github.com/memorax-ai/dsh-patchouli/blob/main/config/providers.schema.json',
    version: 1,
    providers: { local: { kind: 'local', database: 'data/patchouli.db' } },
    routing: { default: 'local', rules: [] },
  }, null, 2)}\n`, { mode: 0o600 })
}

run(source, ['config', 'check', config, '--providers', providers], 'new daemon rejected its configuration')
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

function start(command: string): void {
  const child = spawn(command, [
    'serve',
    '--endpoint', endpoint,
    '--artifacts', artifacts,
    '--providers', providers,
    '--config', config,
  ], {
    detached: true,
    stdio: 'ignore',
    env: { ...process.env, RUNNER_TRACKING_ID: '' },
  })
  child.unref()
}

function waitUntilReady(command: string, timeoutMs: number): boolean {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const result = spawnSync(command, ['status', '--endpoint', endpoint], { encoding: 'utf8' })
    if (result.status === 0) return true
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100)
  }
  return false
}

function run(command: string, args: string[], message: string): void {
  const result = spawnSync(command, args, { encoding: 'utf8', stdio: 'inherit' })
  if (result.status !== 0) throw new Error(message)
}
