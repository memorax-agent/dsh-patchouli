# Development

## Setup

```bash
corepack enable
pnpm install
pnpm check
cargo test --workspace
```

`pnpm check` performs strict TypeScript validation, builds `lib/`, and runs the
Node test suite. The Rust backend is checked independently with Cargo. CI runs
both stacks on Ubuntu, macOS, and Windows.

## Run the daemon shell

Install the Rust CLI into Cargo's binary directory:

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

Manual PowerShell lifecycle:

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
wrappers, containers, and local development. Plugin auto-start launches the
same command as a detached process. Policy validation and the SQLite health
check both complete before the daemon begins listening. CRUD calls are decoded
and routed through `BackendEngine`. The shipped default policy supports
transactional create/read/update/delete. Shared snapshot policies with marker
publication and discard-on-expiry use durable cross-RPC work units. SQLite also
persists causal/session frontiers for immediate-publication policies. Provider
capability mismatches prevent startup rather than failing the first request.

Ctrl-C, Unix SIGTERM, and Windows console shutdown events all use the same
graceful connection drain and provider shutdown path.

Immediate and batch-publication policies may enable keyed idempotency. The
SQLite adapter stores the request and original accepted result atomically with
the entity commit or staged candidate. Retention comes from backend
configuration and is reported by handshake.

Only one daemon may own a SQLite database at a time; ownership is represented
by an exclusive `<database>.lock`. Each successful startup increments a durable
generation and reports whether the prior generation stopped uncleanly.
`checkpoint` waits for a complete WAL checkpoint without stopping the daemon.
`stop` stops accepting connections, closes and drains existing sessions, marks
the generation clean, performs a truncating checkpoint, closes SQLite, and
releases the lock. If the process is terminated instead, SQLite recovers
committed WAL transactions at the next open and the status recovery flag is set.

Validate the existing backend policy file without starting a daemon:

```bash
patchouli-db config check config/patchouli.default.json
patchouli-db config check config/patchouli.default.json --providers config/providers.local.json
```

The two entries in `pnpm-workspace.yaml` are explicit exceptions to pnpm's minimum-release-age policy. DeepSeek Harness and its Cordis dependency were newly published when this repository was initialized; all other dependencies remain subject to the active supply-chain policy.

## CI policy

The repository is public. Pull Request code therefore runs only on GitHub-hosted infrastructure. The registered self-hosted runner is reserved for trusted `main` delivery jobs and explicit manual workflow runs.

The matrix job builds Linux x86_64/aarch64, macOS x86_64/aarch64, and Windows
x86_64 daemon binaries. Tags named `v*` publish those binaries and SHA-256
checksums to a GitHub Release. Trusted `main` and manual runs
also build on the registered self-hosted runner, install the daemon under
`PATCHOULI_DEPLOY_ROOT` (default `~/.patchouli`), restart it, and verify
`patchouli-db status`. A failed health check restores and restarts the previous
binary. Repository variables may override deploy root, endpoint, backend policy,
and provider configuration paths. Without `PATCHOULI_PROVIDERS`, deployment
creates one persistent local-only provider configuration under the deploy root.
The workflow does not install or modify a DSH plugin.

## Generated files

Do not commit `lib/`, package tarballs, runtime databases, or local Harness state. They are covered by `.gitignore` and reproduced by the build or runtime.
