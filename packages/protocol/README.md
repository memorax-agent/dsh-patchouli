# Patchouli Protocol

This package defines the harness-neutral JSON-RPC contract between Patchouli
clients and the database backend. Entity kinds such as knowledge, relations, or
future records are values of the generic `type` parameter; they do not create
separate RPC method families.

This TypeScript package is the client-side binding. The database backend is
implemented in Rust under `crates/backend`.

The language-neutral source of truth is [`openrpc.json`](openrpc.json). Exact
transaction, idempotency, consistency, error, and subscription behavior is
defined in [`SPEC.md`](SPEC.md). TypeScript and Rust tests check their method
identities against the OpenRPC document.

## Scope

Version 1 contains:

- generic entity create, read, retrieve, update, and delete methods;
- chunked upload and download for backend-managed Artifact bytes;
- an open `meta` envelope plus strict CRUD business data;
- configuration-selected preconditions, consistency, causal metadata, and an
  optional conflict-strategy request;
- resumable change subscriptions through opaque cursors;
- cluster and node identity in the connection handshake.

The generic OpenRPC methods do not specialize entity payloads. This package
also publishes the optional, harness-neutral `ArtifactValue`, `KnowledgeValue`,
and `KnowledgeRelationValue` bindings plus their versioned JSON Schemas. Storage
tables, replication, conflict-resolution algorithms, and Harness bindings stay
outside the wire contract.

## Methods

```text
patchouli.protocol.handshake@1
patchouli.control.status@1
patchouli.control.checkpoint@1
patchouli.control.shutdown@1
patchouli.artifact.upload.begin@1
patchouli.artifact.upload.chunk@1
patchouli.artifact.upload.commit@1
patchouli.artifact.download.chunk@1
patchouli.entity.create@1
patchouli.entity.read@1
patchouli.entity.retrieve@1
patchouli.entity.update@1
patchouli.entity.delete@1
patchouli.changes.subscribe@1
patchouli.changes.unsubscribe@1
patchouli.changes.event@1
```

Mutations are JSON-RPC requests and always receive a response. The change event
is a server notification and has no JSON-RPC `id`.

Except for the protocol handshake, every method uses `params: { meta, data }`
and every successful result uses `{ meta, data }`. `data` contains only the
method's business fields. `meta` is an open JSON object whose recognized fields
are selected by backend configuration.

`meta.deadline_unix_ms` is reserved by the protocol as an optional unsigned
Unix-millisecond acceptance deadline. Expiry before a database acceptance commit
returns `DEADLINE_EXCEEDED` and rolls the operation back. Version 1 does not
support request cancellation.

Each create, update, or delete is accepted atomically by the Rust backend
controller. Configuration determines whether the accepted version is published
immediately or as part of a logical batch. Only published versions produce
change notifications.

Artifact upload is a three-step operation: begin, send ordered chunks, then
commit. Commit publishes bytes to the backend-managed content-addressed store
and creates the corresponding `artifact` entity through the same configured
controller path as generic CRUD. Download first reads that entity in the
caller's configured scope, then returns only bytes owned by the serving node.
External `indexed` Artifacts remain generic entities and are read through their
declared source provider.

## Generic entities

An adapter supplies its own entity type and JSON payload vocabulary:

```ts
type EntityType = 'artifact' | 'knowledge' | 'knowledge_relation'
type EntityValue = ArtifactValue | KnowledgeValue | KnowledgeRelationValue
type Contract = PatchouliProtocol<EntityType, EntityValue>
```

The protocol treats `type` as an opaque string. Authorization and schema
validation belong to the backend configuration for that type.

Named identity fields and mixed consistency rules also belong to backend
configuration. See [backend configuration](../../docs/backend-configuration.md).
The frontend plugin does not evaluate these rules or maintain transaction state.

## Concurrency

Every stored version is an opaque string. A deployment may configure a metadata
field containing the versions on which an update or delete is based. A
single-primary backend normally expects one base version; a multi-node backend
may accept several heads without changing the CRUD `data` schema.

Clients must not parse version or causal tokens. When configured, causal input
and output live in `meta` alongside channel, transaction, plugin-route, and
other deployment-specific identities. The controller retains causal progress
for configured identities; CRUD requests do not select a consistency mode. A
configured metadata field may request `merge`, `mvcc`, or `reject` conflict
handling. Its absence uses the backend-configured default.

## Reactive delivery

Subscriptions deliver ordered, at-least-once change notifications. The cursor
is the replay position and is distinct from the causal token. Clients dedupe by
cursor, persist the last applied cursor, and pass it as `after_cursor` after a
reconnect.

The server may reject a cursor outside its retention window with
`CURSOR_EXPIRED`. Disconnecting removes the connection-local subscription but
does not invalidate the cursor.

## Transport binding

The first binding is a UTF-8 NDJSON stream over local IPC. macOS and Linux use
a Unix domain socket; Windows uses a named pipe. One line contains one complete
JSON-RPC object. The same connection carries responses and server
notifications. Batch requests are not part of protocol version 1.
