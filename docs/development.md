# Development

## Setup

```bash
corepack enable
pnpm install
pnpm check
cargo test --workspace
```

`pnpm check` validates and tests the protocol package plus all three TypeScript
frontends. The Rust backend is checked independently with Cargo. CI runs both
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
data directory, and runtime directory without replacing existing files.

Default endpoints are:

- macOS/Linux: `~/.patchouli/run/patchouli.sock`
- Windows: `\\.\pipe\patchouli`

macOS/Linux lifecycle:

```bash
patchouli-db serve \
  --endpoint "$HOME/.patchouli/run/patchouli.sock" \
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
patchouli-db serve --endpoint $endpoint --providers $providers --config $config
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

## Test the DSH frontend

Build an installable package and add the checkout to a local profile:

```bash
pnpm pack
dsh plugin --profile web add .
dsh --profile web --dump-config
```

The default bundle contains:

- `patchouli` → `dsh-patchouli`, registering `ctx.patchouliMemory`;
- `patchouli-agent-loop` → `dsh-patchouli/agent-loop`, registering Hooks and
  Tools.

The storage daemon client is deliberately optional. Add it only for a local
storage-backed MemoryPlugin:

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

The TypeScript storage client covers control, CRUD, entity retrieval, and
cursor-based change subscriptions. Subscription callers register a handler,
persist and deduplicate cursors as needed, and explicitly unsubscribe when their
own lifecycle ends.

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

## Generated files

Do not commit `lib/`, package tarballs, `target/`, runtime databases, or local
Harness state. They are ignored and reproduced by builds or runtime.
