import type {
  DocumentRef,
} from '@ch4acko3/dsh-ui-container/client'
import type { DocumentRenderRequest } from '@ch4acko3/dsh-ui-workspace/client'
import type { KnowledgeScope } from './session-layout.js'
import type { PatchouliMode } from './theme.js'

export type { DocumentRenderRequest } from '@ch4acko3/dsh-ui-workspace/client'

export type AgentSurfaceProps = {
  scope: KnowledgeScope
  mode: PatchouliMode
  activeDocument?: DocumentRef
}

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface SlotMap {
    'patchouli.document.renderer': {
      kind: 'chain'
      scope: 'session'
      owner: DocumentRenderRequest
    }
    'patchouli.agent.surface': {
      kind: 'single'
      scope: 'session'
      owner: AgentSurfaceProps
    }
  }
}
