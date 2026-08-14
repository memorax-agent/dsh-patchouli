# Architecture

## Status

Patchouli currently combines two implemented surfaces:

- a Harness-neutral Rust backend with transactional CRUD, typed knowledge,
  Automerge/MVCC conflict handling, lexical retrieval, retained change streams,
  SQLite and authenticated remote providers, deterministic scope routing, and
  daemon lifecycle recovery;
- a DSH common Memory Service plus an MVP Consumer for the official Agent Loop.

Concrete Memory Plugins are not implemented yet. The optional TypeScript
storage client exposes control and CRUD, but does not yet bridge entity
retrieval or change subscriptions from the backend protocol.

## Goal

Provide DSH with one common memory capability while keeping Agent Loop policy,
memory semantics, and storage mechanics independently replaceable.

```text
Official Agent Loop
  -> dsh-patchouli/agent-loop
  -> ctx.patchouliMemory
  -> registered MemoryPlugin
       |-> MemoraX / another remote API
       `-> optional ctx.patchouli
             <- dsh-patchouli/storage
             -> JSON-RPC daemon
             -> BackendEngine
             -> provider router
             -> SQLite / remote provider
```

## Capability roles

### Common Memory Service

The root `dsh-patchouli` plugin registers `ctx.patchouliMemory`. It owns the
stable high-level `update` / `retrieve` contract, MemoryPlugin registration,
routing, and per-plugin outcomes. It contains no storage, Agent, or prompt
logic.

Current responsibilities:

- accept an opaque scope plus update messages or a retrieval query;
- call every registered MemoryPlugin without interpreting its semantics;
- preserve plugin provenance and isolated failures in aggregate results;
- expose no model-facing schema by itself.

### Memory Plugins

A concrete MemoryPlugin implements both high-level operations. `update` means
submitting information for that plugin to incorporate according to its own
memory semantics; it is not entity replacement. `retrieve` returns
provider-local hits, and scores are not compared across plugins by the common
service.

A MemoraX plugin may call the MemoraX API directly. A local plugin may instead
consume the optional storage client. Storage CRUD types do not appear in the
common Memory Service contract.

### Agent Loop Consumer

`dsh-patchouli/agent-loop` is a separate Cordis plugin. It registers the
model-facing `memory_update` and `memory_retrieve` tools, listens to
`agent/pre-step` for automatic retrieval, and can observe committed
`session/event` turn boundaries for automatic update. Scope comes from the
session working directory, falling back to the session id, rather than model
input.

The retrieval Hook calls `next()`, extracts text only from directly sourced
user messages, retrieves through the common service, and appends one
plugin-sourced recall message. Tool continuations, rejected steps, empty
results, and retrieval failures inject nothing.

Automatic update is opt-in. A `completed` or `max-tokens` `turn/end` is the
post-commit boundary; the Consumer reconstructs that bracket from the canonical
Session Log and submits direct-user plus assistant text once. Plugin-injected
context, reasoning, tool calls, and tool results are excluded. Updates are
serialized per Session, failures do not affect the Agent Loop, and Consumer
disposal aborts and drains admitted work.

Every call includes normalized metadata identifying the official Agent Loop,
the trigger (`manual-tool`, `pre-step`, or `turn-end`), and available
session/turn/step position. MemoryPlugins therefore do not depend on Agent or
Session objects.

### Optional Storage Client

`dsh-patchouli/storage` registers `ctx.patchouli`, connects to an existing
daemon, and may start one when the configured endpoint is unavailable. It is
not part of the default bundle, so remote-only MemoryPlugins do not require a
local daemon.

The client currently exposes status, checkpoint, and generic entity
create/read/update/delete. The backend protocol also implements entity retrieve
and cursor-based change subscriptions, but the TypeScript client does not yet
expose those methods or dispatch `patchouli.changes.event@1` notifications.

The daemon remains independent of plugin lifecycle: unloading the storage
plugin closes its IPC connection but does not administratively stop the daemon.
Shutdown goes through the `patchouli stop` CLI.

### Storage Backend

The Rust backend owns persistence plus generic entity CRUD, retrieval, and
change streams. It does not depend on Agent lifecycle, prompt assembly, DSH, or
Cordis. Its first fact vocabulary contains versioned `knowledge` and
`knowledge_relation` entities; see [the fact model](knowledge-model.md).

The frontend binding remains stateless with respect to database policy:

```text
JSON-RPC adapter
    -> backend controller
    -> configured policy engine
    -> provider boundary
```

The controller owns schemas, identity extraction, consistency planning,
logical work units, conflicts, idempotency, and publication. Selected behavior
compiles metadata aliases into scoped snapshot, acquisition, session, and
commit-order constraints. Rules are alternatives; constraints inside one
behavior combine without fallback or downgrade.

`BackendEngine` owns immutable validated policy and one injected provider
boundary. That boundary may be a router containing the required local SQLite
authority and named remote authorities. Cross-request business facts live in
the provider; the engine retains no process-local transaction or batch map.

Providers also own durable lifecycle. Startup takes exclusive database
ownership, lets SQLite recover committed state, records a runtime generation,
and only then exposes IPC. Graceful shutdown drains connections, records a clean
stop, checkpoints WAL, closes the provider, and releases ownership. Every
mutation is made durable by its provider transaction before its RPC response
succeeds.

Database providers are compile-time Rust adapters:

- `patchouli-provider` defines the common boundary;
- `patchouli-provider-sqlite` owns local persistence;
- `patchouli-provider-remote` transports provider primitives over authenticated
  HTTPS;
- `patchouli-provider-router` maps canonical scope JSON to one named provider.

Routing uses first-match order and one explicit default. It never falls back to
another provider after failure, and one atomic work unit cannot cross routes.
`cluster_id` and `node_id` identify a serving process but do not imply
replication.

The IPC framing is shared across platforms: Unix domain sockets on macOS/Linux,
Windows named pipes, and UTF-8 NDJSON on every platform.

## Model interface

The official Consumer currently provides automatic retrieval and explicit
update/retrieve tools. Automatic retrieval defaults on; committed-turn
automatic update defaults off. Turn-count, character-count, and inactivity
policies remain follow-up work in the Consumer rather than provider-specific
behavior in the common service.

## Delivery status

Completed:

1. Common Memory Service and MemoryPlugin registry.
2. Official Agent Loop Consumer with Hook and Tool paths.
3. Harness-neutral storage protocol, transactional daemon, retrieval, change
   stream, and local/remote providers.
4. Optional TypeScript control and CRUD client.

Next:

1. A MemoraX MemoryPlugin working end to end through the common service.
2. A local storage-backed MemoryPlugin.
3. TypeScript entity retrieve and change-subscription bridging.
4. Operational memory surfaces such as inspection and rebuild.

The root package is the DSH frontend bundle. Rust backend crates live under
`crates/`, and only `packages/protocol` is a Harness-neutral TypeScript package.
