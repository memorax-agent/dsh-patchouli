# @memorax-agent/dsh-patchouli-workspace-indexer

Cordis plugin boundary for indexing DeepSeek Harness workspaces through
`ctx.workspaceRegistry` and `ctx.fs` into the in-process
`ctx.patchouliMemory` service.

The package currently declares its runtime dependencies only. Crawling,
watching, and update behavior will be implemented separately.
