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
```

The Cordis plugin defaults to `autoStart: true` and invokes the `patchouli`
binary from `PATH` when its endpoint is unavailable. It opens the SQLite file
at `~/.patchouli/data/patchouli.db` and loads backend policy from
`~/.patchouli/config.json` by default. Copy the development policy before the
first automatic start:

```bash
mkdir -p "$HOME/.patchouli"
cp config/patchouli.default.json "$HOME/.patchouli/config.json"
```

Default endpoints are:

- macOS/Linux: `~/.patchouli/run/patchouli.sock`
- Windows: `\\.\pipe\patchouli`

Manual macOS/Linux lifecycle:

```bash
patchouli serve \
  --endpoint "$HOME/.patchouli/run/patchouli.sock" \
  --database "$HOME/.patchouli/data/patchouli.db" \
  --config "$HOME/.patchouli/config.json"
patchouli status --endpoint "$HOME/.patchouli/run/patchouli.sock"
patchouli checkpoint --endpoint "$HOME/.patchouli/run/patchouli.sock"
patchouli stop --endpoint "$HOME/.patchouli/run/patchouli.sock"
```

Manual PowerShell lifecycle:

```powershell
$endpoint = '\\.\pipe\patchouli'
$database = Join-Path $HOME '.patchouli\data\patchouli.db'
$config = Join-Path $HOME '.patchouli\config.json'
patchouli serve --endpoint $endpoint --database $database --config $config
patchouli status --endpoint $endpoint
patchouli checkpoint --endpoint $endpoint
patchouli stop --endpoint $endpoint
```

`serve` remains in the foreground for launchd, systemd, Windows Service
wrappers, containers, and local development. Plugin auto-start launches the
same command as a detached process. Policy validation and the SQLite health
check both complete before the daemon begins listening. CRUD calls are decoded
and routed through `BackendEngine`; until transactional storage is implemented,
they return `UNSUPPORTED_CAPABILITY` rather than reporting success.

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
patchouli config check config/patchouli.default.json
```

The two entries in `pnpm-workspace.yaml` are explicit exceptions to pnpm's minimum-release-age policy. DeepSeek Harness and its Cordis dependency were newly published when this repository was initialized; all other dependencies remain subject to the active supply-chain policy.

## Test the bundle locally

```bash
dsh plugin --profile web add .
dsh --profile web --dump-config
```

The effective configuration should contain an enabled row with id `patchouli` and module name `dsh-patchouli`.

## CI policy

The repository is public. Pull Request code therefore runs only on GitHub-hosted infrastructure. The registered self-hosted runner is reserved for trusted `main` delivery jobs and explicit manual workflow runs.

The delivery job creates an npm tarball and uploads it as a workflow artifact. It does not mutate a local DSH installation. Server installation will be added only after the following values are fixed:

- target runner labels;
- target DSH profile;
- persistent release directory;
- health check and rollback command.

## Generated files

Do not commit `lib/`, package tarballs, runtime databases, or local Harness state. They are covered by `.gitignore` and reproduced by the build or runtime.
