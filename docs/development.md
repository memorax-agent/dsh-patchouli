# Development

Product code and documentation are maintained on separate branches. The
`main` branch owns the monorepo, runtime, protocol, backend, and tests. The
`docs` branch owns only this VitePress site. Unless a section explicitly says
otherwise, commands on this page run from a product checkout of `main`.

## Product setup

```bash
git clone --branch main --single-branch https://github.com/memorax-agent/dsh-patchouli.git
cd dsh-patchouli
corepack enable
pnpm install
pnpm check
cargo test --workspace
```

`pnpm check` validates and tests every TypeScript workspace. The Rust backend
is checked independently with Cargo. CI runs both
stacks on Ubuntu, macOS, and Windows.

The entries in `pnpm-workspace.yaml` are explicit exceptions to pnpm's
minimum-release-age policy for newly published DeepSeek Harness packages; all
other dependencies remain subject to the active supply-chain policy.

## Run the daemon shell

Install the Rust CLI:

```bash
cargo install --path crates/server
patchouli-db init --root "$HOME/.patchouli"
```

The daemon opens the SQLite file at `~/.patchouli/data/patchouli.db` and loads
backend policy from `~/.patchouli/config.json` in the default local layout.
DeepSeek Harness/Cordis startup integration is maintained by the frontend
collaborator. `init` creates the default policy, local provider configuration,
SQLite data directory, managed Artifact store, and runtime directory without
replacing existing files.

Default endpoints are:

- macOS/Linux: `~/.patchouli/run/patchouli.sock`
- Windows: `\\.\pipe\patchouli`

macOS/Linux lifecycle:

```bash
patchouli-db serve \
  --endpoint "$HOME/.patchouli/run/patchouli.sock" \
  --artifacts "$HOME/.patchouli/data/artifacts" \
  --providers "$HOME/.patchouli/providers.json" \
  --config "$HOME/.patchouli/config.json"
patchouli-db status --endpoint "$HOME/.patchouli/run/patchouli.sock"
patchouli-db checkpoint --endpoint "$HOME/.patchouli/run/patchouli.sock"
patchouli-db stop --endpoint "$HOME/.patchouli/run/patchouli.sock"
```

PowerShell lifecycle:

```powershell
$endpoint = '\\.\pipe\patchouli'
$config = Join-Path $HOME '.patchouli\config.json'
$providers = Join-Path $HOME '.patchouli\providers.json'
$artifacts = Join-Path $HOME '.patchouli\data\artifacts'
patchouli-db serve --endpoint $endpoint --artifacts $artifacts --providers $providers --config $config
patchouli-db status --endpoint $endpoint
patchouli-db checkpoint --endpoint $endpoint
patchouli-db stop --endpoint $endpoint
```

`serve` remains in the foreground for launchd, systemd, Windows Service
wrappers, containers, and local development. The optional storage plugin starts
the same command as a detached process when `autoStart` is enabled. Policy
validation and provider health checks finish before the daemon listens.

The default policy supports transactional create/read/update/delete, generic
retrieval, retained change streams, durable cross-RPC work units, keyed
idempotency, and persisted causal/session frontiers. Provider capability
mismatches prevent startup instead of failing the first request.

Ctrl-C, Unix SIGTERM, and Windows console shutdown events all use the same
graceful connection drain and provider shutdown path.

Immediate and batch-publication policies may enable keyed idempotency. The
SQLite adapter stores the request and original accepted result atomically with
the entity commit or staged candidate. Retention comes from backend
configuration and is reported by handshake.

Only one daemon may own a local SQLite database at a time. A successful startup
increments a durable generation and reports whether the previous generation
stopped uncleanly. `checkpoint` flushes WAL without stopping; `stop` drains
connections, marks the generation clean, truncates WAL, closes SQLite, and
releases the lock. SQLite recovers committed WAL transactions after an abnormal
termination.

Validate configuration without starting the daemon:

```bash
patchouli-db config check config/patchouli.default.json
patchouli-db config check config/patchouli.default.json --providers config/providers.local.json
```

Run the real third-party plugin loop against a temporary SQLite database:

```bash
pnpm test:e2e
```

This command builds `patchouli-db`, starts a temporary daemon, mounts the common
Memory Service, storage client, Artifact Ingestor, and private CRUD test plugin,
then verifies direct and DSH-sourced managed Artifact upload/download plus
create, read, retrieve, update, and delete through the public services. The CRUD
test plugin is not part of the default bundle.

## Test the DSH frontend

Build an installable package and add the checkout to a local profile:

```bash
pnpm pack
dsh plugin --profile web add .
dsh --profile web --dump-config
```

The default bundle contains:

- `patchouli` → `dsh-patchouli`, registering `ctx.patchouliMemory`;
- `patchouli-agent-loop` → `@memorax-agent/dsh-patchouli-agent-loop`,
  registering individually configurable Agent/Session/Tool Hooks and the two
  optional model Tools;
- `patchouli-artifact-ingestor` →
  `@memorax-agent/dsh-patchouli-artifact-ingestor`, reading DSH attachment and
  workspace-file bytes when `ctx.patchouli`, `ctx.attachments`, and `ctx.fs` are
  available;
- `patchouli-session-indexer` →
  `@memorax-agent/dsh-patchouli-session-indexer`, currently declaring
  `patchouliMemory + sessionQuery` only;
- `patchouli-workspace-indexer` →
  `@memorax-agent/dsh-patchouli-workspace-indexer`, currently declaring
  `patchouliMemory + workspaceRegistry + fs` only;
- `patchouli-memory-cursors` → `dsh-patchouli/cursor-store`, registering
  `ctx.patchouliMemoryCursors` when `storageDomain` is available.

The Web bundle supplies `storageDomain`, so the cursor service opens its
`patchouli_memory` domain and persists progress through the configured DSH
storage backend. The standard Headless bundle has no storage stack: the
cursor-service fiber remains pending on its injection, while the independent
Memory Service, Agent Loop, Tools, and update/retrieve paths still load. Add the
DSH storage stack for durable Headless subscriptions, or pass a custom
`MemoryCursorStore` to `ctx.patchouliMemory.subscribe`.

Bind one cursor store for each logical Consumer subscription:

```ts
const cursorStore = ctx.patchouliMemoryCursors.bind({
  consumerId: 'example-consumer',
  subscriptionKey: 'memory-changes',
  scope,
})

const subscription = await ctx.patchouliMemory.subscribe(
  {
    meta: {
      source: { type: 'consumer', id: 'example-consumer' },
      scope,
    },
  },
  change => applyChangeIdempotently(change),
  { cursorStore, signal, onError: reportSubscriptionError },
)
```

The binding key is `(consumerId, subscriptionKey, scope, pluginId)`. Scope and
cursors remain opaque. The service saves the provider boundary first, handles
one plugin's events serially, deduplicates the current cursor, and advances it
only after the Consumer handler succeeds. Plugins run independently.

Only a `MemorySubscriptionError` marked `retryable` reconnects, with bounded
exponential backoff and the last durable cursor. Fatal or unknown errors stop
that plugin worker. `resetRequired` also stops and reports without deleting the
cursor: complete snapshot/resync first, unsubscribe the old high-level handle,
call `cursorStore.delete(pluginId)`, then create a replacement subscription.
Explicit unsubscribe, the Consumer fiber lifecycle, and its AbortSignal all
cancel retries, cancel provider handles, and drain admitted handlers.

The storage daemon client is deliberately optional. Add it only for a local
storage-backed MemoryPlugin:

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

The TypeScript storage client covers control, CRUD, entity retrieval, and
cursor-based change subscriptions. JSON-RPC failures are `PatchouliRpcError`
instances retaining `method`, `code`, `data`, and the optional protocol
`reason`. A subscription handle exposes the server boundary plus `closed`,
which resolves with `unsubscribed`, `connection-lost`, or `client-closed`, and
an idempotent `unsubscribe()`.

The low-level handler is invoked in wire order but is not awaited or serialized
by the client. A storage-backed MemoryPlugin must serialize application and map
connection loss or `CURSOR_EXPIRED` into the appropriate high-level
`MemorySubscriptionError`; the common service then owns cursor persistence,
deduplication, retry, and Consumer lifecycle draining.

## CI policy

The repository is public. Pull Request code runs only on GitHub-hosted
infrastructure. The registered self-hosted runner is reserved for trusted
`main` delivery jobs and explicit manual runs.

The matrix job checks Node and Rust and builds Linux x86_64/aarch64, macOS
x86_64/aarch64, and Windows x86_64 daemon binaries. Trusted `main` and manual
runs also package the protocol and DSH plugin as artifacts. `v*` tags publish
the daemon binaries and SHA-256 checksums to a GitHub Release. Trusted `main`
and manual runs can install the daemon under `PATCHOULI_DEPLOY_ROOT` (default
`~/.patchouli`), restart it, and verify `patchouli-db status`; a failed health
check restores and restarts the previous binary. Repository variables may
override deploy root, endpoint, backend policy, and provider configuration
paths. Without `PATCHOULI_PROVIDERS`, deployment creates one persistent
local-only provider configuration under the deploy root. The workflow never
installs or modifies a DSH profile.

## Documentation site

Use an independent checkout of the `docs` branch when editing this site:

```bash
git clone --branch docs --single-branch \
  https://github.com/memorax-agent/dsh-patchouli.git dsh-patchouli-docs
cd dsh-patchouli-docs
corepack enable
pnpm install
pnpm docs:dev
```

The development server exposes an architecture-diagram editor on the
Architecture page. Click **Edit diagram**, or open
`/architecture?edit-architecture=1` directly. Choose the desktop or mobile
layout, then edit module positions and dimensions, the Harness boundary,
localized module copy, directed-edge annotations, route visibility, and the
canvas height for each layout. Each directed edge also has an editable line
type, arrow style and size, and color. You can add or delete modules and edges,
move modules into or out of the Harness boundary, change edge endpoints and
layout-specific handles, or reverse an edge. Deleting a module also removes its
incident edges. Modules can be repositioned directly on the canvas. **Save to
source** validates the result and atomically updates
`docs/components/patchouli-architecture.data.json`; the editor and write
endpoint are absent from production builds.

Run `pnpm docs:build` before committing documentation changes. Product build
commands and source files are intentionally unavailable in this checkout.

## Generated files

Do not commit `lib/`, package tarballs, `target/`, runtime databases, or local
Harness state. They are ignored and reproduced by builds or runtime.
