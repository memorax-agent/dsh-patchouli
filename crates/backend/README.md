# Patchouli Backend

Rust implementation boundary for Patchouli persistence. This crate is
independent of DeepSeek Harness and Cordis.

The runnable process shell lives in `crates/server`. It currently exposes only
handshake, status, and shutdown over Unix sockets on macOS/Linux and named
pipes on Windows; storage methods remain unimplemented.

The current layer contains the backend-service CRUD contract, wire-compatible
request and response types, the configuration model, and the reactive
change-stream contract. The RPC adapter calls a Rust backend controller through
`BackendService`. The controller uses `PolicySelector` to derive scope,
baseline, idempotency, and publication keys, persists control state, and only
then uses a database provider's storage primitives.

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
