# Architecture

## Status

Patchouli currently provides an installable Cordis plugin, a local Rust daemon,
cross-platform IPC bootstrap, control CLI, a database-provider boundary, and a
SQLite startup adapter. A validated `BackendEngine` is connected to the daemon
and CRUD methods reach its explicit placeholder implementation. Transactional
CRUD execution, retrieval, and context injection are not implemented yet.

## Goal

Provide DeepSeek Harness with local, workspace-aware knowledge context without requiring the model to decide to call a knowledge tool.

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

- accept a query, workspace identity, result limit, and cancellation signal;
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
injected provider. JSON-RPC sessions call it only through `BackendService`.
Cross-request business facts will live in the provider; the engine retains no
process-local transaction or batch map.

The provider also owns the durable daemon lifecycle. Startup takes exclusive
database ownership, lets the database recover committed state, records a new
runtime generation, and only then exposes IPC. A manual checkpoint flushes WAL
pages without stopping service. Graceful shutdown first drains IPC connections,
then records a clean stop, truncates the WAL, closes the provider, and releases
ownership. A later startup reports whether the previous generation ended
uncleanly. This runtime marker does not replace business transactions: once
CRUD is implemented, every mutation must become durable through its own
provider transaction before its RPC response succeeds.

Database providers are compile-time Rust adapters rather than dynamically
loaded plugins. `patchouli-provider` owns the common contract,
`patchouli-provider-sqlite` owns SQLite connection details, and the daemon
library accepts the contract through dependency injection. The shipped CLI
composes SQLite; another executable may choose another adapter without changing
the JSON-RPC protocol or Cordis service. SQLite is the default server Cargo
feature and can be omitted when embedding the server library. Exactly one
provider is selected at startup and failure is explicit—there is no provider
fallback chain.

Physical provider settings are not part of the business-policy schema. The
SQLite path is a daemon startup option, while entity schemas and consistency
rules remain in backend policy configuration.

### Runtime bootstrap

The root Cordis plugin registers `ctx.patchouli`, connects to an existing local
daemon, and optionally starts one when the configured endpoint is unavailable.
The daemon remains independent of the plugin lifecycle: unloading the plugin
closes its IPC connection but does not stop the daemon. Administrative shutdown
goes through the `patchouli stop` CLI.

The IPC framing and JSON-RPC dispatcher are shared across platforms:

- macOS and Linux: Unix domain socket;
- Windows: named pipe;
- all platforms: UTF-8 NDJSON with one JSON-RPC object per line.

### Context Consumer

The Consumer listens to `agent/pre-step`, calls `next()`, retrieves against the admitted user messages, and appends one plugin-sourced user message to the returned decision.

The injected message must be:

- bounded by an explicit result and byte budget;
- tagged with plugin provenance and section metadata;
- appended rather than inserted into earlier history;
- admitted through the normal Session Log path so the model-visible request can be reconstructed;
- emitted once per user turn unless a later requirement justifies another retrieval.

## Default model interface

Patchouli will not expose knowledge retrieval as a model tool by default. Human commands, administration UI, or optional tools may be added as separate Consumers only when there is a concrete use case.

## Initial delivery sequence

1. A single local Provider and automatic Context Consumer working end to end.
2. Durable ingestion and incremental indexing.
3. Operational surfaces such as status, rebuild, and inspection.

The repository is a monorepo. Rust backend crates live under `crates/`; JavaScript/TypeScript protocol and Harness packages live under `packages/`. The root package remains the DeepSeek Harness plugin until it is moved into its own package.
