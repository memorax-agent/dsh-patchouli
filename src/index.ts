import { homedir } from 'node:os'
import { join } from 'node:path'
import process from 'node:process'

import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'

import { PatchouliService } from './service.js'

export { PatchouliService } from './service.js'

/** Cordis plugin identity used by loader diagnostics and model provenance. */
export const name = 'dsh-patchouli'

/** The daemon binding does not depend on Harness services. */
export const inject = [] as const

export interface Config {
  /** Unix socket path or Windows named-pipe name shared with the CLI. */
  endpoint: string
  /** Patchouli executable used when auto-start is enabled. */
  command: string
  /** SQLite database file passed to a daemon started by this plugin. */
  databasePath: string
  /** Backend policy configuration loaded by a daemon started by this plugin. */
  backendConfigPath: string
  /** Start a detached daemon when no existing daemon can be reached. */
  autoStart: boolean
  /** Maximum time to wait for a newly started daemon. */
  startupTimeoutMs: number
}

const defaultEndpoint = process.platform === 'win32'
  ? String.raw`\\.\pipe\patchouli`
  : join(homedir(), '.patchouli', 'run', 'patchouli.sock')

export const Config: z<Config> = z.object({
  endpoint: z.string().default(defaultEndpoint),
  command: z.string().default('patchouli'),
  databasePath: z.string().default(join(homedir(), '.patchouli', 'data', 'patchouli.db')),
  backendConfigPath: z.string().default(join(homedir(), '.patchouli', 'config.json')),
  autoStart: z.boolean().default(true),
  startupTimeoutMs: z.natural().min(1).default(5_000),
})

/** Register a lifecycle-managed daemon client on `ctx.patchouli`. */
export async function apply(ctx: Context, config: Config): Promise<() => void> {
  const service = new PatchouliService(ctx, config)
  await service.start()
  return () => service.close()
}
