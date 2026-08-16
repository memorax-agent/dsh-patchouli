/** Cordis plugin identity used by loader diagnostics. */
export const name = 'dsh-patchouli-workspace-indexer'

/** Workspace indexing uses DSH workspace metadata and filesystem access. */
export const inject = ['patchouliMemory', 'workspaceRegistry', 'fs'] as const

/** Workspace indexing behavior will be added behind this package boundary. */
export function apply(): void {}
