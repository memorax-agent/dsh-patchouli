import type { Context } from '@deepseek-ai/cordis'

/** Cordis plugin identity used by loader diagnostics and model provenance. */
export const name = 'dsh-patchouli'

/** The bootstrap plugin has no service dependencies yet. */
export const inject = [] as const

/**
 * Install Patchouli into a Cordis context.
 *
 * Knowledge services and model-visible context are introduced only when their
 * end-to-end implementation is present; the bootstrap plugin intentionally
 * has no runtime side effects.
 */
export function apply(_ctx: Context): void {}
