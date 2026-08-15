# Architecture

## Status

Patchouli currently combines two implemented surfaces:

- a Harness-neutral Rust backend with transactional CRUD, typed knowledge,
  Automerge/MVCC conflict handling, lexical retrieval, retained change streams,
  SQLite and authenticated remote providers, deterministic scope routing, and
  daemon lifecycle recovery;
- a DSH common Memory Service with reactive subscriptions, durable Consumer
  cursors, and an MVP Consumer for the official Agent Loop.

Concrete Memory Plugins are not implemented yet. The optional TypeScript
storage client exposes control, CRUD, entity retrieval, structured RPC errors,
and cursor-based change subscriptions from the backend protocol.

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

Reactive Consumer
  -> ctx.patchouliMemory.subscribe
  -> ctx.patchouliMemoryCursors
  -> DSH storageDomain
```

## Capability roles

### Common Memory Service

The root `dsh-patchouli` plugin registers `ctx.patchouliMemory`. It owns the
stable high-level `update` / `retrieve` / `subscribe` contract, MemoryPlugin
registration, routing, and per-plugin provenance. It contains no storage,
Agent, or prompt logic.

Current responsibilities:

- accept an opaque scope plus update messages or a retrieval query;
- call every registered MemoryPlugin without interpreting its semantics;
- preserve plugin provenance and isolated failures in aggregate results;
- subscribe the current snapshot of plugins that implement the optional
  reactive contract;
- serialize change application per plugin while allowing plugins to progress
  independently;
- expose no model-facing schema by itself.

### Memory Plugins

A concrete MemoryPlugin implements both high-level operations and may implement
reactive `subscribe`. `update` means submitting information for that plugin to
incorporate according to its own memory semantics; it is not entity
replacement. `retrieve` returns provider-local hits, and scores are not
compared across plugins by the common service.

A MemoraX plugin may call the MemoraX API directly. A local plugin may instead
consume the optional storage client. Storage CRUD types do not appear in the
common Memory Service contract.

For a subscription, a plugin receives the opaque scope, optional metadata, and
the Consumer's last durable cursor as `afterCursor`. It returns a boundary
cursor, a `closed` promise, and `unsubscribe`. The common service persists the
boundary before admitting queued events, compares cursors only for equality,
deduplicates the last applied cursor, invokes one plugin's handler serially,
and saves the new cursor only after the handler succeeds. Consumers must still
apply changes idempotently because a crash may occur between their side effect
and cursor persistence.

Only `MemorySubscriptionError` with `retryable: true` triggers reconnect. The
service uses exponential backoff from 250 ms to 30 seconds plus jitter and
reopens from the last saved cursor. Unknown, fatal, and `resetRequired` errors
stop only that plugin worker and reach the Consumer through `onError`; other
plugins continue. A reset never silently deletes progress. The Consumer first
performs an explicit snapshot/resync, then replaces the old subscription,
deletes that plugin cursor, and subscribes again.

The high-level subscription handle lists its plugin snapshot. Its idempotent
`unsubscribe` aborts retry waits, cancels live provider handles, and drains
admitted handlers. The same cleanup runs for an owning Cordis fiber or an
external AbortSignal. `closed` resolves after this overall cleanup, rather than
when one plugin worker fails.

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

### Durable Consumer Cursors

`dsh-patchouli/cursor-store` registers `ctx.patchouliMemoryCursors`. `bind`
validates and binds `consumerId`, `subscriptionKey`, and the opaque scope; its
returned `MemoryCursorStore` adds the plugin id, so records are isolated by the
four-part identity. The service stores only an opaque cursor and deliberately
does not own Consumer snapshot or reset policy.

The default bundle includes this plugin. A Web assembly supplies
`storageDomain`, so its cursors are durable in the configured DSH storage
backend. A Headless assembly without the storage stack leaves the injected
cursor-store fiber pending. Cordis dependency isolation means the common Memory
Service, Agent Loop Consumer, and update/retrieve paths remain available.
Headless reactive Consumers can load the storage stack or provide another
`MemoryCursorStore` directly.

### Optional Storage Client

`dsh-patchouli/storage` registers `ctx.patchouli`, connects to an existing
daemon, and may start one when the configured endpoint is unavailable. It is
not part of the default bundle, so remote-only MemoryPlugins do not require a
local daemon.

The client exposes status, checkpoint, generic entity create/read/retrieve/
update/delete, and cursor-based subscriptions. Daemon JSON-RPC failures retain
their method, numeric code, data, and optional protocol reason in
`PatchouliRpcError`. A caller registers one handler per subscription; the
client dispatches `patchouli.changes.event@1` notifications to it in wire order.

The returned handle adds `closed`, which resolves with `unsubscribed`,
`connection-lost`, or `client-closed` and an optional transport error. Its
`unsubscribe` is idempotent. Unsubscribe and connection closure both remove the
handler. This low-level client does not serialize asynchronous handlers,
persist cursors, or reconnect; a storage-backed MemoryPlugin maps those
mechanics into the high-level subscription contract.

The daemon remains independent of plugin lifecycle: unloading the storage
plugin closes its IPC connection but does not administratively stop the daemon.
Shutdown goes through the `patchouli-db stop` CLI.

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

For a routed provider, control status reports the maximum recovery generation
and marks recovery after an unclean shutdown when any configured provider
reports one. Lifecycle errors always identify the provider that failed.

## Model interface

The official Consumer currently provides automatic retrieval and explicit
update/retrieve tools. Automatic retrieval defaults on; committed-turn
automatic update defaults off. Turn-count, character-count, and inactivity
policies remain follow-up work in the Consumer rather than provider-specific
behavior in the common service.

## Delivery status

Completed:

1. Common Memory Service, MemoryPlugin registry, reactive routing, and durable
   Consumer cursor binding.
2. Official Agent Loop Consumer with Hook and Tool paths.
3. Harness-neutral storage protocol, transactional daemon, retrieval, change
   stream, and local/remote providers.
4. Optional TypeScript control, CRUD, retrieval, and lifecycle-aware
   change-subscription client.

Next:

1. A MemoraX MemoryPlugin working end to end through the common service.
2. A local storage-backed MemoryPlugin.
3. Operational memory surfaces such as inspection and rebuild.

The root package is the DSH frontend bundle. Rust backend crates live under
`crates/`, and only `packages/protocol` is a Harness-neutral TypeScript package.
