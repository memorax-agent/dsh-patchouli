import { homedir } from 'node:os'
import { join } from 'node:path'
import process from 'node:process'

import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'

import { PatchouliStorageService } from './service.js'

export {
  PatchouliRpcError,
  PatchouliStorageService,
  type ChangeHandler,
  type ChangeSubscriptionClose,
  type ChangeSubscriptionHandle,
  type EntityQueryOptions,
  type WorkUnitMutation,
} from './service.js'

/** Optional Cordis plugin that binds the Patchouli storage daemon. */
export const name = 'dsh-patchouli-storage'

/** The daemon binding does not depend on Harness services. */
export const inject = [] as const

export interface Config {
  /** Unix socket path or Windows named-pipe name shared with the CLI. */
  endpoint: string
  /** Patchouli executable used when auto-start is enabled. */
  command: string
  /** Provider and scope-routing configuration used by an auto-started daemon. */
  providerConfigPath: string
  /** Backend policy configuration used by an auto-started daemon. */
  backendConfigPath: string
  /** Backend-managed content-addressed artifact directory. */
  artifactRootPath: string
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
  command: z.string().default('patchouli-db'),
  providerConfigPath: z.string().default(join(homedir(), '.patchouli', 'providers.json')),
  backendConfigPath: z.string().default(join(homedir(), '.patchouli', 'config.json')),
  artifactRootPath: z.string().default(join(homedir(), '.patchouli', 'data', 'artifacts')),
  autoStart: z.boolean().default(true),
  startupTimeoutMs: z.natural().min(1).default(5_000),
})

/** Register a lifecycle-managed daemon client on `ctx.patchouliStorage`. */
export async function apply(ctx: Context, config: Config): Promise<() => void> {
  const service = new PatchouliStorageService(ctx, config)
  await service.start()
  return () => service.close()
}
