# Architecture

## Status

Patchouli provides a local Rust daemon, cross-platform IPC, a control CLI, an
explicit database-provider boundary, a SQLite adapter, authenticated remote
provider transport, and deterministic scope routing. Transactional CRUD,
durable shared-baseline work units, Automerge/MVCC resolution, immediate and
batch idempotency, lexical retrieval, causal/session frontiers, retained change
streams, and lifecycle recovery are implemented. Harness/Cordis integration
and model context injection are maintained outside the backend scope.

## Goal

Provide any Harness with a local, workspace-aware knowledge store through one
stable process boundary, without binding storage behavior to that Harness.

```text
Knowledge Provider
       │
       ▼
Knowledge Service on ctx
       │
       ▼
Context Consumer ── agent/pre-step ── Session Log ── Model request
```

## Capability roles

### Service Definition

The Service Definition owns the stable query contract and shared result vocabulary. It contains no storage, indexing, Agent, or prompt logic.

Expected responsibilities:

- accept a query, workspace identity, result limit, and optional acceptance deadline;
- return bounded hits with stable identifiers, source locations, content, and scores;
- expose no model-facing schema by itself.

### Service Provider

The database backend is implemented in Rust and owns persistence plus the generic entity CRUD/change stream. It must not depend on Agent lifecycle, prompt assembly, DeepSeek Harness, or Cordis. Provider and retrieval code consume this backend through the versioned JSON-RPC contract.

The first fact vocabulary has two configured entity kinds: `knowledge` and
`knowledge_relation`. Their versioned JSON Schemas and Rust/TypeScript bindings are
harness-neutral and do not add knowledge-specific RPC methods. Generic entity
identity/version remains outside the value, while SQLite stores immutable
versions once and exposes typed read-only views. See
[the fact model](knowledge-model.md).

The frontend binding is stateless with respect to database policy. Backend calls follow this boundary:

```text
JSON-RPC adapter
    -> backend controller
    -> configured policy engine
    -> database provider primitives
```

The controller owns schemas, identity extraction, consistency planning, logical
transaction/batch state, conflicts and publication. A selected behavior
compiles metadata aliases into scoped snapshot, acquisition, session, and
commit-order constraints. Rules are alternatives; constraints inside one
behavior combine without fallback or downgrade. A database provider does not
interpret business fields and is never called directly by the frontend adapter.

`BackendEngine` is the runtime owner of the immutable validated policy and one
injected provider boundary. That boundary may be a routing provider containing
the required `local` SQLite authority and named remote authorities. JSON-RPC sessions call it only through `BackendService`.
Cross-request business facts live in the provider; the engine retains no
process-local transaction or batch map.

The provider also owns the durable daemon lifecycle. Startup takes exclusive
database ownership, lets the database recover committed state, records a new
runtime generation, and only then exposes IPC. A manual checkpoint flushes WAL
pages without stopping service. Graceful shutdown first drains IPC connections,
then records a clean stop, truncates the WAL, closes the provider, and releases
ownership. A later startup reports whether the previous generation ended
uncleanly. This runtime marker does not replace business transactions: every
mutation becomes durable through its own provider transaction before its RPC
response succeeds.

Database providers are compile-time Rust adapters rather than dynamically
loaded plugins. `patchouli-provider` owns the common contract,
`patchouli-provider-sqlite` owns SQLite connection details,
`patchouli-provider-remote` transports the same provider primitives over an
authenticated HTTPS boundary, and `patchouli-provider-router` maps canonical
scope JSON to exactly one named provider. Rules use first-match order followed
by one explicit default; routing never falls back after provider failure. One
atomic work unit cannot cross routes.

`cluster_id` and `node_id` identify the serving process but do not imply
replication. Multi-node support enters through a provider that advertises a
replica source and causal frontier operations; the engine validates those
capabilities before startup. SQLite remains a single authority and never
pretends that a local transaction makes remote replicas immediately visible.

Physical provider settings are not part of the business-policy schema. A
separate provider configuration defines `local`, remote endpoints and ordered
scope routes, while entity schemas and consistency rules remain in backend
policy configuration. Remote storage nodes expose provider primitives rather
than a second backend engine, so policy and conflict logic execute once.

### External runtime bootstrap

The frontend-owned Cordis plugin may register `ctx.patchouli`, connect to an existing local
daemon, and optionally starts one when the configured endpoint is unavailable.
The daemon remains independent of the plugin lifecycle: unloading the plugin
closes its IPC connection but does not stop the daemon. Administrative shutdown
goes through the `patchouli-db stop` CLI.

For a routed provider, control status reports the maximum recovery generation
and marks recovery after an unclean shutdown when any configured provider
reports one. Lifecycle errors always identify the provider that failed.

The IPC framing and JSON-RPC dispatcher are shared across platforms:

- macOS and Linux: Unix domain socket;
- Windows: named pipe;
- all platforms: UTF-8 NDJSON with one JSON-RPC object per line.

### External Context Consumer

The Consumer listens to `agent/pre-step`, calls `next()`, retrieves against the admitted user messages, and appends one plugin-sourced user message to the returned decision.

The injected message must be:

- bounded by an explicit result and byte budget;
- tagged with plugin provenance and section metadata;
- appended rather than inserted into earlier history;
- admitted through the normal Session Log path so the model-visible request can be reconstructed;
- emitted once per user turn unless a later requirement justifies another retrieval.

## Default model interface

Patchouli will not expose knowledge retrieval as a model tool by default. Human commands, administration UI, or optional tools may be added as separate Consumers only when there is a concrete use case.

## Delivery sequence

1. A single local Provider and stable CRUD/retrieval/change protocol.
2. Durable ingestion and incremental indexing.
3. Operational surfaces such as status, rebuild, and inspection.

The repository is a monorepo. Rust backend crates live under `crates/`, and the
Harness-neutral TypeScript protocol lives under `packages/protocol`. Harness
packages may consume the protocol but are not part of backend architecture.
