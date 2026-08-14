# Development

## Setup

```bash
corepack enable
pnpm install
pnpm check
cargo test --workspace
```

`pnpm check` performs strict TypeScript validation, builds `lib/`, and runs the Node test suite.
The Rust backend is checked independently with Cargo.

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
