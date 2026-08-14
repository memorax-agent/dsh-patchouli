# Patchouli Backend

Rust implementation boundary for Patchouli persistence. This crate is
independent of DeepSeek Harness and Cordis.

The runnable process shell lives in `crates/server`. It exposes handshake,
control, and typed CRUD routing over Unix sockets on macOS/Linux and named pipes
on Windows. The default single-node policy executes CRUD through the provider's
transactional snapshot and compare-and-swap primitives.

The current layer contains `BackendEngine`, the backend-service CRUD contract,
wire-compatible request and response types, the configuration model, and the
reactive change-stream contract. `PolicySelector` already derives scoped
consistency and effective conflict plans, including request-level conflict
overrides. The conflict resolver implements Automerge for configured JSON
fields and MVCC variants for other data. Engine startup retains a validated
immutable policy and a healthy injected provider. Shared snapshot plus marker
publication runs as a durable work unit. SQLite-backed causal/session frontiers,
immediate and batch keyed idempotency, retained change streams, and lexical
retrieval are executable. Configuration version 1 deliberately exposes only
marker close plus discard-on-expiry rather than advertising unfinished modes.

Entity kinds remain opaque to the CRUD protocol. The backend additionally
publishes the first harness-neutral fact vocabulary: typed `knowledge` and
`knowledge_relation` Rust values backed by versioned JSON Schemas. They use the
same methods and storage interface as any future configured kind; see
[`docs/knowledge-model.md`](../../docs/knowledge-model.md).

The frontend plugin is stateless with respect to database policy. JSON Schema,
identity extraction, batching, phase-specific consistency constraints, and
conflicts are backend configuration; see
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
