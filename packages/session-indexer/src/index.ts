/** Cordis plugin identity used by loader diagnostics. */
export const name = 'dsh-patchouli-session-indexer'

/** Session indexing depends on the DSH query surface and Patchouli memory. */
export const inject = ['patchouli', 'sessionQuery'] as const

/** Session indexing behavior will be added behind this package boundary. */
export function apply(): void {}
