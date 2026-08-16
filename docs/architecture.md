# Architecture

## Status

Patchouli currently combines two implemented surfaces:

- a Harness-neutral Rust backend with transactional CRUD, typed knowledge,
  Automerge/MVCC conflict handling, lexical retrieval, retained change streams,
  SQLite and authenticated remote providers, deterministic scope routing, and
  daemon lifecycle recovery;
- a DSH common Memory Service with reactive subscriptions, durable Consumer
  cursors, an official Agent Loop adapter, managed Artifact ingestion, and
  package boundaries for Session and Workspace Indexers.

Session and Workspace Indexer behavior and general Knowledge extraction/
retrieval MemoryPlugins are not implemented yet. The optional TypeScript
storage client exposes control, CRUD, entity retrieval, Artifact transfer,
structured RPC errors, and cursor-based change subscriptions from the backend
protocol.

## Goal

Provide DSH with one common memory capability while keeping Agent Loop policy,
memory semantics, and storage mechanics independently replaceable.

```text
Official Agent Loop
  -> @memorax-agent/dsh-patchouli-agent-loop
  -> ctx.patchouliMemory
  -> registered MemoryPlugin
       |-> MemoraX / another remote API
       |-> artifact-ingestor -> attachments / fs
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

DSH sessionQuery                  DSH workspaceRegistry + fs
  -> session-indexer                -> workspace-indexer
  -> ctx.patchouliMemory            -> ctx.patchouliMemory
```

`ctx.patchouliMemory` is an in-process DSH service. Patchouli does not expose
it through an external bridge; other Harnesses and external applications need
their own adapters.

## Capability roles

### Common Memory Service

The root `dsh-patchouli` plugin registers `ctx.patchouliMemory`. It owns the
stable high-level `update` / `retrieve` / `subscribe` contract, MemoryPlugin
registration, routing, and per-plugin provenance. It contains no storage,
Agent, or prompt logic.

Current responsibilities:

- accept a common call envelope containing trusted source identity, opaque
  scope, optional JSON attributes, and operation-specific arguments;
- evaluate each MemoryPlugin's optional synchronous filter against
  `operation + meta`, with no filter preserving broadcast behavior;
- omit plugins that do not match and expose filter exceptions as isolated
  per-plugin routing failures;
- preserve plugin provenance and isolated failures in aggregate results;
- subscribe the matching snapshot of plugins that implement the optional
  reactive contract;
- serialize change application per plugin while allowing plugins to progress
  independently;
- expose no model-facing schema by itself.

### Memory Plugins

A concrete MemoryPlugin implements both high-level operations and may implement
reactive `subscribe`. `update` means submitting information for that plugin to
incorporate according to its own memory semantics; it is not entity
replacement. Both operations accept source-owned JSON `data` and return
plugin-owned JSON data. The common service does not interpret either shape; it
only adds plugin provenance and isolates failures in the aggregate result.

Callers never select plugin ids. A plugin may pass a synchronous, pure filter
when registering; the filter sees only the operation and common `meta` fields.
This keeps routing policy with the plugin that understands its own capability
while allowing unfiltered third-party plugins to retain broadcast behavior.

A MemoraX plugin may call the MemoraX API directly. A local plugin may instead
consume the optional storage client. Storage CRUD types do not appear in the
common Memory Service contract.

`@memorax-agent/dsh-patchouli-artifact-ingestor` is the narrow local exception:
it is an update-only MemoryPlugin that converts DSH resource references into
managed Artifact entities. The high-level call still contains JSON only. For
session images the plugin resolves `ImageAttachmentRef` through
`ctx.attachments`; for explicit `workspace-file` requests it resolves the path
through `ctx.fs`, verifies canonical containment in the Session workspace, and
applies a configured byte limit. It then transfers bytes through
`ctx.patchouli.uploadArtifact`. The plugin remains pending unless all three DSH
services and the optional storage client are present.

The private `@memorax-agent/dsh-patchouli-crud-test-plugin` package exercises
that boundary without becoming a production memory implementation. It routes
only calls from the `crud-test` source, selects one storage CRUD method from
`meta.attributes.operation`, passes the call's `data` to `ctx.patchouli`
unchanged, and returns the daemon JSON unchanged. It is excluded from the
default DSH bundle.

For a subscription, a plugin receives the same call `meta` and the Consumer's
last durable cursor as `afterCursor`. It returns a boundary
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

`@memorax-agent/dsh-patchouli-agent-loop` is a separate Cordis plugin. It registers the
model-facing `memory_update` and `memory_retrieve` tools and maps selected
official Agent, Session, and Tool extension points onto the common service.
Each point has an independent switch. Retrieval is available at
`agent/session-start`, `agent/pre-step`, `agent/turn-stopping`, and
`tools/post-execute`; storage is available at `agent/created`, `agent/disposed`,
`agent/request-error`, `agent/error`, committed `session/turn-end`, and
`tools/result`. The defaults enable `agent/pre-step`, `session/turn-end`, and
both model tools only.

The adapter is a transport boundary, not a memory implementation. It snapshots
all lossless JSON facts visible at the selected point, including the Agent
identity and options, Session header and relevant events, Hook payload, or Tool
execution and outcome. It does not construct retrieval prompts, extract text,
summarize observations, or decide what should become memory. The source-owned
payload is passed as `data`; each registered MemoryPlugin interprets it.
Successful retrieval outcomes are represented as one plugin-sourced JSON recall
message per MemoryPlugin. `tools/post-execute` appends them to the official
`additionalContexts` channel, pre-step appends them to the accepted step, and
the turn-stopping/session-start points inject them through the Agent.

`agent/session-start` is an official fire-and-forget notification, so its
retrieval is best-effort and injected when ready. Pre-step, turn-stopping, and
post-execute preserve their awaited official Hook semantics. Updates are
serialized per Session; `session/flush` waits for admitted update work, and
consumer disposal aborts and drains all admitted background work.

Every call includes normalized `meta`. Its source is
`{ type: "agent-loop", id: "dsh-patchouli-agent-loop" }`; scope comes from the
Session working directory and falls back to the Session id. JSON attributes
carry `point`, `sessionId`, and any available turn/step/outcome position. The
model cannot supply this trusted envelope, and the common service never exposes
Agent, Session, or Tool runtime objects to MemoryPlugins.

The model-facing `memory_update` Tool also accepts JSON `workspace-file`
resource descriptors. It does not open paths itself. This preserves the same
adapter boundary while allowing the Artifact Ingestor to perform the trusted
DSH filesystem lookup. DSH image content already carries a durable attachment
reference in Session events and is discovered from the committed turn.

### Indexer Packages

`@memorax-agent/dsh-patchouli-session-indexer` depends on
`ctx.patchouliMemory` and `ctx.sessionQuery`. It will own session scanning and
incremental submission, but currently contains no indexing behavior.

`@memorax-agent/dsh-patchouli-workspace-indexer` depends on
`ctx.patchouliMemory`, `ctx.workspaceRegistry`, and `ctx.fs`. It will own
workspace crawling and change observation, but currently contains no indexing
behavior. Missing DSH services leave the corresponding Cordis plugin pending;
the packages do not substitute fallback data sources.

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
update/delete, managed Artifact upload/download, and cursor-based subscriptions.
Daemon JSON-RPC failures retain their method, numeric code, data, and optional
protocol reason in `PatchouliRpcError`. A caller registers one handler per
subscription; the client dispatches `patchouli.changes.event@1` notifications
to it in wire order.

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
Cordis. Its first fact vocabulary contains versioned `artifact`, `knowledge`,
and `knowledge_relation` entities; see [the fact model](knowledge-model.md).

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
boundary. Its fact vocabulary represents managed files and external local or
remote indexes as the same `artifact` entity; Knowledge stores only typed
Artifact references. That boundary may be a router containing the required local SQLite
authority and named remote authorities. Cross-request business facts live in
the provider; the engine retains no process-local transaction or batch map.

Providers also own durable lifecycle. Startup takes exclusive database
ownership, lets SQLite recover committed state, records a runtime generation,
and only then exposes IPC. Graceful shutdown drains connections, records a clean
stop, checkpoints WAL, closes the provider, and releases ownership. Every
mutation is made durable by its provider transaction before its RPC response
succeeds.

The daemon-managed Artifact store is a separate local content-addressed file
library. Upload commit publishes verified bytes and then creates the scoped
Artifact entity through `BackendEngine`; download resolves that entity first.
The database remains authoritative for visibility and policy, while the file
library owns byte storage and never exposes its physical paths.

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
2. Official Agent Loop adapter with Hook and Tool paths.
3. Harness-neutral storage protocol, transactional daemon, retrieval, change
   stream, and local/remote providers.
4. Optional TypeScript control, CRUD, retrieval, and lifecycle-aware
   change-subscription client.
5. DSH attachment and explicit workspace-file ingestion into managed Artifact
   storage.
6. Independent Session and Workspace Indexer package boundaries.

Next:

1. Session and Workspace Indexer behavior.
2. A MemoraX MemoryPlugin working end to end through the common service.
3. A local Knowledge extraction and retrieval MemoryPlugin.
4. Operational memory surfaces such as inspection and rebuild.

The root package is the DSH frontend bundle. Rust backend crates live under
`crates/`; `packages/protocol` is Harness-neutral, while the other TypeScript
packages are DSH adapters, plugins, and indexers.
