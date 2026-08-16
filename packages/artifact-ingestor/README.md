# @memorax-agent/dsh-patchouli-artifact-ingestor

Storage-backed MemoryPlugin that turns DSH image attachment references and
explicit Agent Loop `workspace-file` resources into managed Patchouli Artifact
entities.

The plugin keeps bytes outside `ctx.patchouliMemory`: it resolves trusted DSH
references through `ctx.attachments` or `ctx.fs`, checks workspace containment,
then uploads verified bytes through `ctx.patchouli`. It requires the Patchouli
storage client, which the bundled DSH profile enables by default.

```yaml
ingestSessionImages: true
maxFileBytes: 33554432
metaFields:
  scope: workspace_id
  source: user_id
  session: channel_id
fixedMeta: {}
```

`metaFields` maps the high-level Memory call identity onto the deployment's
configured backend metadata names. `fixedMeta` supplies additional string
identity fields required by that deployment. `maxFileBytes` applies to both
workspace files and DSH image attachments.
