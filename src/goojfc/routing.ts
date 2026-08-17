import type { MemoryRouteFilter } from '../memory.js'

import { supportsAgentLoopPoints } from './adapters/input.js'

const preStep = ['agent/pre-step'] as const
const memoryEvolveRetrieval = supportsAgentLoopPoints({ retrieve: preStep })

/** Provider-owned semantic boundary shared by adapters and async bridge proxies. */
export const goojfcRouteFilters = {
  openviking: supportsAgentLoopPoints({
    update: ['agent/created', 'agent/disposed', 'session/turn-end'],
    retrieve: ['agent/session-start', ...preStep],
  }),
  hindsight: supportsAgentLoopPoints({
    update: ['session/turn-end'],
    retrieve: preStep,
  }),
  memos: supportsAgentLoopPoints({
    update: ['agent/disposed', 'session/turn-end'],
    retrieve: preStep,
  }),
  mneme: supportsAgentLoopPoints({
    update: ['session/turn-end'],
    retrieve: preStep,
  }),
  mnemon: supportsAgentLoopPoints({ retrieve: preStep }),
  'memory-gate': supportsAgentLoopPoints({
    update: ['session/turn-end'],
    retrieve: preStep,
  }),
  lingshu: supportsAgentLoopPoints({
    update: ['session/turn-end'],
    retrieve: preStep,
  }),
  'graph-memory': supportsAgentLoopPoints({ retrieve: preStep }),
  engramory: supportsAgentLoopPoints({ retrieve: preStep }),
  'memory-evolve': (call => call.operation === 'retrieve' && memoryEvolveRetrieval(call)),
} satisfies Record<string, MemoryRouteFilter>
