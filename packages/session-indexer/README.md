# dsh-patchouli-session-indexer

Cordis plugin boundary for indexing DeepSeek Harness sessions through
`ctx.sessionQuery` into the in-process `ctx.patchouli` service.

The package currently declares its runtime dependencies only. Scanning,
incremental progress, and update behavior will be implemented separately.
