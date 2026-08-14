import type {} from '@deepseek-ai/cordis'
import type {
  DocumentActionRegistry,
  ExplorerPaneRegistry,
} from '@ch4acko3/dsh-ui-workspace/client'
import type { ExplorerPaneContext } from './PreviewExplorerPanes.js'
import type { FilterRegistry } from './filters.js'

export type MemoryUiContributions = {
  explorerPanes: ExplorerPaneRegistry<ExplorerPaneContext>
  documentActions: DocumentActionRegistry
  filters: FilterRegistry
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    patchouliMemoryUi: MemoryUiContributions
  }
}
