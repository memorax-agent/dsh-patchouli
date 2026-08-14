import type { ClientContext } from '@deepseek-ai/dsh-client-runtime/client'
import * as memoryUi from '@memorax-agent/dsh-memory-ui/client'

export const name = 'dsh-patchouli'
export const inject = [] as const

/** Mount Patchouli after its independently installed UI dependencies are active. */
export function apply(ctx: ClientContext): void {
  ctx.plugin(memoryUi)
}

export * from '@ch4acko3/dsh-ui-container/client'
export * from '@ch4acko3/dsh-ui-workspace/client'
export * from '@memorax-agent/dsh-memory-ui/client'
