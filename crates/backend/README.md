# Patchouli Backend

Rust implementation boundary for Patchouli persistence. This crate is
independent of DeepSeek Harness and Cordis.

The runnable process shell lives in `crates/server`. It exposes handshake,
control, and typed CRUD routing over Unix sockets on macOS/Linux and named pipes
on Windows. Storage methods currently return an explicit
`UNSUPPORTED_CAPABILITY` placeholder.

The current layer contains `BackendEngine`, the backend-service CRUD contract,
wire-compatible request and response types, the configuration model, and the
reactive change-stream contract. Engine startup retains a validated immutable
policy and a healthy injected provider. The next storage layer will use
`PolicySelector` to derive scope, baseline, idempotency, and publication keys
before invoking provider transaction primitives.

Entity kinds are opaque strings. `memory`, `relation`, and future kinds all use
the same methods and storage interface.

The frontend plugin is stateless with respect to database policy. JSON Schema,
identity extraction, batching, baselines, timestamps, and conflicts are backend
configuration; see
[`docs/backend-configuration.md`](../../docs/backend-configuration.md).
Every non-handshake RPC crosses this boundary as `{ meta, data }`: the
controller interprets configured `meta` fields while the provider receives
explicit storage operations.

## Checks

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
