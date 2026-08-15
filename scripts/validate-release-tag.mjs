import { readFileSync } from 'node:fs'

const manifest = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8')
const workspace = manifest.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/)
const version = workspace?.[1].match(/^version\s*=\s*"([^"]+)"/m)?.[1]

if (!version) {
  throw new Error('workspace package version is missing')
}

const expected = `v${version}`
if (process.env.GITHUB_REF_NAME !== expected) {
  throw new Error(`release tag ${process.env.GITHUB_REF_NAME} must equal ${expected}`)
}
