import type { Context } from '@deepseek-ai/cordis'
import { MemoryService } from './memory.js'

export * from './memory.js'

/** Cordis plugin identity used by loader diagnostics and model provenance. */
export const name = 'dsh-patchouli'

/** The common memory frontend has no service dependencies. */
export const inject = [] as const

/**
 * Install the common Patchouli memory frontend into a Cordis context.
 */
export function apply(ctx: Context): void {
  ctx.plugin(MemoryService)
}
