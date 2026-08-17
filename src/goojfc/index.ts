import type { Context } from '@deepseek-ai/cordis'

import { createEngramoryAdapter } from './adapters/engramory.js'
import { createGraphMemoryAdapter } from './adapters/graph-memory.js'
import { createHindsightAdapter } from './adapters/hindsight.js'
import { createLingshuAdapter } from './adapters/lingshu.js'
import { createMemoryEvolveAdapter } from './adapters/memory-evolve.js'
import { createMemoryGateAdapter } from './adapters/memory-gate.js'
import { createMemosAdapter } from './adapters/memos.js'
import { createMnemeAdapter } from './adapters/mneme.js'
import { createMnemonAdapter } from './adapters/mnemon.js'
import { createOpenVikingAdapter } from './adapters/openviking.js'
import {
  bridgeServices,
  installBridge,
  type GoojfcService,
} from './bridge.js'

export {
  bridgeServices,
  type BridgeServiceName,
  type GoojfcService,
} from './bridge.js'
export {
  createHindsightAdapter,
  type HindsightAdapterNative,
} from './adapters/hindsight.js'
export {
  createMemosAdapter,
  type MemosAdapterOptions,
} from './adapters/memos.js'
export {
  createMnemeAdapter,
  type MnemeService,
  type MnemeSummarizer,
} from './adapters/mneme.js'
export {
  createMnemonAdapter,
  type MnemonLifecycle,
  type MnemonRuntime,
  type MnemonService,
} from './adapters/mnemon.js'
export {
  createMemoryGateAdapter,
  type MemoryGateAdapterOptions,
  type MemoryGateService,
} from './adapters/memory-gate.js'
export {
  createLingshuAdapter,
  type LingshuBridge,
  type LingshuCaptureConfig,
} from './adapters/lingshu.js'
export {
  createGraphMemoryAdapter,
  type GraphMemoryNative,
} from './adapters/graph-memory.js'
export {
  createEngramoryAdapter,
  type EngramoryAdapterOptions,
} from './adapters/engramory.js'
export {
  createMemoryEvolveAdapter,
  type MemoryEvolveNative,
} from './adapters/memory-evolve.js'
export {
  createOpenVikingAdapter,
  type OpenVikingClient,
  type OpenVikingRuntime,
} from './adapters/openviking.js'

/** Temporary compatibility patches for pre-Patchouli knowledge plugins. */
export const name = 'dsh-patchouli-goojfc'

/** Compatibility adapters participate in Patchouli's common memory frontend. */
export const inject = ['patchouli'] as const
export const provide = 'patchouliGoojfc'

const service = {
  createOpenVikingAdapter,
  createHindsightAdapter,
  createMemosAdapter,
  createMnemeAdapter,
  createMnemonAdapter,
  createMemoryGateAdapter,
  createLingshuAdapter,
  createGraphMemoryAdapter,
  createEngramoryAdapter,
  createMemoryEvolveAdapter,
} satisfies GoojfcService

export function apply(ctx: Context): void {
  ctx.provide(provide, service)
  for (const serviceName of bridgeServices) installBridge(ctx, serviceName)
}
