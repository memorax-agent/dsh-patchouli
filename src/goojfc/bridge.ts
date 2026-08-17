import type { Context } from '@deepseek-ai/cordis'

import type { MemoryPlugin } from '../memory.js'
import type { MemoryRouteCall } from '../memory.js'
import type { createEngramoryAdapter } from './adapters/engramory.js'
import type { createGraphMemoryAdapter } from './adapters/graph-memory.js'
import type { createHindsightAdapter } from './adapters/hindsight.js'
import type { createLingshuAdapter } from './adapters/lingshu.js'
import type { createMemoryEvolveAdapter } from './adapters/memory-evolve.js'
import type { createMemoryGateAdapter } from './adapters/memory-gate.js'
import type { createMemosAdapter } from './adapters/memos.js'
import type { createMnemeAdapter } from './adapters/mneme.js'
import type { createMnemonAdapter } from './adapters/mnemon.js'
import type { createOpenVikingAdapter } from './adapters/openviking.js'
import { goojfcRouteFilters } from './routing.js'

export const bridgeServices = [
  'goojfcOpenViking',
  'goojfcHindsight',
  'goojfcMemos',
  'goojfcMneme',
  'goojfcMnemon',
  'goojfcMemoryGate',
  'goojfcLingshu',
  'goojfcGraphMemory',
  'goojfcEngramory',
  'goojfcMemoryEvolve',
] as const

export type BridgeServiceName = typeof bridgeServices[number]

export interface GoojfcService {
  readonly createOpenVikingAdapter: typeof createOpenVikingAdapter
  readonly createHindsightAdapter: typeof createHindsightAdapter
  readonly createMemosAdapter: typeof createMemosAdapter
  readonly createMnemeAdapter: typeof createMnemeAdapter
  readonly createMnemonAdapter: typeof createMnemonAdapter
  readonly createMemoryGateAdapter: typeof createMemoryGateAdapter
  readonly createLingshuAdapter: typeof createLingshuAdapter
  readonly createGraphMemoryAdapter: typeof createGraphMemoryAdapter
  readonly createEngramoryAdapter: typeof createEngramoryAdapter
  readonly createMemoryEvolveAdapter: typeof createMemoryEvolveAdapter
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    patchouliGoojfc: GoojfcService
    goojfcOpenViking: MemoryPlugin
    goojfcHindsight: MemoryPlugin
    goojfcMemos: MemoryPlugin
    goojfcMneme: MemoryPlugin
    goojfcMnemon: MemoryPlugin
    goojfcMemoryGate: MemoryPlugin
    goojfcLingshu: MemoryPlugin
    goojfcGraphMemory: MemoryPlugin
    goojfcEngramory: MemoryPlugin
    goojfcMemoryEvolve: MemoryPlugin
  }
}

export function installBridge(ctx: Context, serviceName: BridgeServiceName): void {
  ctx.inject([serviceName], (bridgeCtx) => (
    bridgeCtx.patchouli.register(bridgeCtx[serviceName], {
      filter: call => bridgeFilter(serviceName, call),
    })
  ))
}

function bridgeFilter(serviceName: BridgeServiceName, call: MemoryRouteCall): boolean {
  return bridgeFilters[serviceName](call)
}

const bridgeFilters: Record<BridgeServiceName, (call: MemoryRouteCall) => boolean> = {
  goojfcOpenViking: goojfcRouteFilters.openviking,
  goojfcHindsight: goojfcRouteFilters.hindsight,
  goojfcMemos: goojfcRouteFilters.memos,
  goojfcMneme: goojfcRouteFilters.mneme,
  goojfcMnemon: goojfcRouteFilters.mnemon,
  goojfcMemoryGate: goojfcRouteFilters['memory-gate'],
  goojfcLingshu: goojfcRouteFilters.lingshu,
  goojfcGraphMemory: goojfcRouteFilters['graph-memory'],
  goojfcEngramory: goojfcRouteFilters.engramory,
  goojfcMemoryEvolve: goojfcRouteFilters['memory-evolve'],
}
