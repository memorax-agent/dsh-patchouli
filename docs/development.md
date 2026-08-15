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

## Test the plugins locally

```bash
dsh plugin --profile web add github:CH4ACKO3/dsh-ui-container
dsh plugin --profile web add github:CH4ACKO3/dsh-ui-workspace
dsh plugin --profile web add ./packages/memory-ui
dsh plugin --profile web add .
dsh --profile web --dump-config
```

The effective configuration should contain enabled `patchouli` and `patchouli-memory-ui` rows. UI Container and UI Workspace must also be direct dependencies of the same profile.

See [package and plugin conventions](packages.md) before adding another workspace package.

## CI policy

The repository is public. Pull Request code therefore runs only on GitHub-hosted infrastructure. The registered self-hosted runner is reserved for trusted `main` delivery jobs and explicit manual workflow runs.

The delivery job creates one npm tarball for every publishable package and uploads them together as a workflow artifact. It does not mutate a local DSH installation. Server installation will be added only after the following values are fixed:

- target runner labels;
- target DSH profile;
- persistent release directory;
- health check and rollback command.

## Generated files

Do not commit `lib/`, package tarballs, runtime databases, or local Harness state. They are covered by `.gitignore` and reproduced by the build or runtime.
