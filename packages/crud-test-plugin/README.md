# @memorax-agent/dsh-patchouli-crud-test-plugin

Test-only third-party MemoryPlugin used to verify the complete Patchouli path:

```text
ctx.patchouliMemory -> MemoryPlugin -> ctx.patchouli -> daemon -> provider
```

The package is private and is not part of the default DSH bundle. It accepts
calls whose `meta.source.type` is `crud-test`. Set
`meta.attributes.operation` to `create`, `read`, `retrieve`, `update`, or
`delete`; pass the matching backend RPC params as `data`. The plugin forwards
that `data` object to the storage client unchanged and returns its JSON result
unchanged.

Mutations use `ctx.patchouliMemory.update`; reads use
`ctx.patchouliMemory.retrieve`.
