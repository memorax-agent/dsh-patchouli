# Getting started

Patchouli contains two independently usable surfaces:

- a DeepSeek Harness plugin bundle that exposes the common Memory Service;
- the optional `patchouli-db` daemon for transactional local or remote storage.

The database backend is not required when every registered MemoryPlugin uses
its own external service.

## Requirements

- Node.js `^22.19.0` or `>=24.0.0`
- pnpm 11
- Rust stable when building `patchouli-db` from source
- a DeepSeek Harness runtime compatible with `0.1.0-rc.6`

## Install the DSH plugin from a checkout

```bash
corepack enable
pnpm install
pnpm pack
dsh plugin --profile web add .
dsh --profile web --dump-config
```

The Web profile should include the core `patchouli` plugin, Agent Loop adapter,
Artifact Ingestor, Session and Workspace Indexer boundaries, and durable memory
cursor service.

## Install the optional backend

::: code-group

```bash [macOS / Linux]
curl -fsSL https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.sh | sh
```

```powershell [Windows PowerShell]
irm https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.ps1 | iex
```

:::

The installer verifies release checksums and initializes `~/.patchouli` without
overwriting existing configuration. It does not modify a DSH profile, register a
system service, or start a background process. See [Backend
installation](./installation.md) for platform details and source builds.

## Enable the storage client

The storage client is intentionally excluded from the default bundle. Enable it
only for plugins that need the Patchouli daemon:

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

The client exposes daemon control, generic entity CRUD, retrieval, managed
Artifact transfer, and cursor-based change subscriptions through
`ctx.patchouli`.

## Next steps

- Configure Agent Loop hooks and consumers in [DSH integration](./dsh-integration.md).
- Understand process and plugin boundaries in [Architecture](./architecture.md).
- Select identity and consistency rules in [Backend configuration](./backend-configuration.md).
- Model files, knowledge, and relations with the [Knowledge model](./knowledge-model.md).
