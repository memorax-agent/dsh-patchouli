# Patchouli Storage Protocol v1

Status: draft 7. Normative method and data schemas are in `openrpc.json`.

The words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY are normative.

## 1. Transport and session

- The local transport is a UTF-8, newline-delimited JSON-RPC 2.0 byte stream.
- macOS and Linux use a Unix domain socket. Windows uses a named pipe.
- One line MUST contain exactly one complete JSON-RPC object. JSON string line
  breaks therefore remain escaped inside that object.
- Batch requests are not supported in v1.
- Request IDs MUST be strings or integers. Clients MUST NOT reuse an ID while
  its request is outstanding on the same connection.
- The client MUST complete `patchouli.protocol.handshake@1` before any other
  Patchouli request on a connection.
- The request `capabilities` array lists capabilities supported by the client.
  The result `capabilities` array lists the negotiated subset supported by both
  peers. Both arrays contain unique capability names.
- A server notification has no `id`. Clients MUST NOT respond to
  `patchouli.changes.event@1`.
- Every non-handshake method uses `params: { meta, data }`. `meta` is an open
  JSON object interpreted by backend configuration apart from the reserved
  `deadline_unix_ms` field. `data` is the strict method business schema; entity
  `value` remains open JSON.

Method suffix `@1` is part of method identity. A breaking request, response, or
behavior change requires a new method suffix.

### Control methods

`patchouli.control.status@1` reports daemon readiness, selected database
provider, runtime generation, unclean-shutdown recovery, process identity,
start time, and active connection count. `patchouli.control.checkpoint@1` asks
the provider to checkpoint durable state without stopping. `patchouli.control.shutdown@1`
accepts a graceful local shutdown request. All follow the normal `{ meta, data }`
shape; their `data` request object is empty.

## 2. Identity and scope

The wire-level entity reference is `(type, id)`.

- `type` is an opaque, non-empty string. Values such as `artifact`, `knowledge`, and
  `knowledge_relation`
  do not create different protocol methods.
- `id` is an opaque, non-empty string. When create omits it, the server MUST
  generate it and return it in the accepted entity reference.
- Version and cursor tokens are opaque. Clients may store and compare them for
  equality but MUST NOT parse or order them.

Configured metadata may namespace this reference with tenant, workspace,
channel, transaction, time, plugin, causal, or other identities. Metadata used
for authorization or uniqueness MUST be required by backend configuration.
`node_id` identifies only the serving process.

## 3. Backend controller and configuration

The client is stateless with respect to storage policy. It forwards generic CRUD
requests and does not interpret schema, transaction groups, timestamps,
replicas, or conflict rules.

The RPC server MUST pass each request to a backend controller before invoking a
database provider. The controller owns schema validation, named-field
extraction, consistency-rule selection, logical baselines, durable batch state,
conflict handling, publication, and response construction. A database provider
exposes storage primitives and MUST NOT independently reinterpret business
fields.

Configuration MAY define:

- named fields extracted from `meta` and validated by their own JSON Schemas;
- a global scope that namespaces `(type, id)` and change records;
- a JSON Schema for each entity type's `value`;
- ordered rules selected by metadata presence;
- phase-specific snapshot acquisition, session, and commit-ordering constraints;
- separate consistency, idempotency, conflict, and publication keys and policies.

Channel IDs, transaction IDs, application timestamps, causal tokens, idempotency keys,
plugin-route IDs, base versions, and conflict-strategy requests remain ordinary
configured metadata. They MUST NOT create new RPC methods or
hard-coded protocol fields. Consistency selection is exclusively
backend-controller policy; only conflict handling has a request override.

## 4. Transaction and idempotency semantics

Each create, update, and delete request is accepted using one short database
transaction. The following effects MUST commit atomically:

1. the immutable entity candidate or tombstone;
2. the idempotency record and serialized successful result, when enabled;
3. assignment to the configured logical group, if any.

Without a configured deferred batch, the same transaction MUST publish the
visible head and change record. With deferred visibility, accepted candidates
MUST remain hidden from ordinary reads and subscriptions until the configured
close condition fires. Closing publishes all accepted heads and change records
in one short database transaction. The controller MUST NOT keep a database
transaction or connection open between RPC calls.

A successful mutation response means durable acceptance. Publication may be
immediate or deferred. A stateless client observes publication through a causal
read or the change stream; it does not maintain batch state itself.

When the selected behavior enables keyed idempotency, the controller derives
its identity from configured `meta` fields. Repeating the same identity and
mutation data MUST return the original accepted result without another
candidate or change event. Reusing the identity with different mutation data
MUST return `IDEMPOTENCY_CONFLICT`. The guarantee lasts at least
`idempotency_retention_seconds` reported by the handshake. Immediate and
deferred acceptance both store the accepted result atomically. With idempotency
disabled, clients MUST NOT assume that retrying a mutation is deduplicated.

`meta.deadline_unix_ms`, when present, is an unsigned Unix-millisecond
acceptance deadline reserved by the protocol and is not a configurable identity
field. Expiry before acceptance commits MUST roll back and return
`DEADLINE_EXCEEDED`. Once acceptance commits, the operation is successful even
if the response is delivered after the deadline; a retry with the same
idempotency identity obtains the stored result. Protocol v1 does not support
request cancellation. For remote providers, the authority provider's clock is
authoritative at the commit boundary, so participating nodes must keep their
system clocks synchronized.

v1 does not expose a transaction lifecycle in JSON-RPC. Cross-request grouping
is controller state driven by configuration and version 1 uses an explicit
marker close condition. Request snapshot mode reads one
database snapshot per request. Shared snapshot mode fixes one baseline for the
configured scoped group, exposes that group's staged overlay to its own reads,
and requires batch publication with the same grouping fields.

Closing compares every mutated entity's published heads with the group's fixed
baseline. Drift is resolved per entity using the conflict strategy persisted
with that staged mutation: `reject` returns `VERSION_CONFLICT`, `mvcc` retains
both branches, and `merge` applies the configured CRDT rules and fallback. The
resolved set is published with one atomic compare-and-swap; a write racing that
final compare returns entity-specific conflict details. A discard-on-expiry
group permanently rejects further use of that identity and publishes none of
its staged mutations.

## 5. Versions and conflicts

The selected conflict policy identifies a base-version metadata field. For
update and delete that field MUST contain a non-empty set without duplicates.
It declares the versions on which the candidate is based. The policy also
identifies an optional metadata field containing `merge`, `mvcc`, or `reject`;
when absent, backend configuration supplies the strategy:

- `reject` requires exact equality with the complete current head set and
  otherwise returns `VERSION_CONFLICT`;
- `mvcc` accepts concurrent candidates as multiple heads;
- `merge` applies backend-configured CRDT rules and uses the configured fallback
  for fields or discriminator groups outside those rules;
- unknown base versions return `VERSION_CONFLICT` and the current set;
- an unknown entity identity returns `NOT_FOUND`.

The shipped Knowledge policy applies Automerge to `/content`, groups it by the
relative `/kind` discriminator, and applies MVCC to all other fields. A request
still carries one complete JSON value. CRDT changes and frontiers are internal
database state, not a second mutation format.

A backend may contain multiple heads after replication. A read returns:

- `active`: exactly one active head;
- `deleted`: exactly one tombstone head;
- `conflicted`: any other non-empty head combination.

Submitting every current head through the configured metadata field lets the
controller resolve a conflict to the single version created by the mutation.

Delete creates a tombstone; it does not make the identity indistinguishable
from one that never existed. A never-existing identity returns `NOT_FOUND`.

## 6. Consistency

Consistency is a conjunction of constraints compiled from the selected backend
behavior. Metadata fields supply identities, tokens, or bounds; they do not
select a client-controlled consistency level.

- request snapshots have no persistent baseline identity;
- shared snapshots use `scope + configured key fields` and remain fixed for the
  lifetime of the durable transaction group;
- allowed source sets are intersected by acquisition requirements;
- causal and session frontiers are joined into the minimum acceptable frontier;
- linearizable acquisition requires an authority ordering point between request
  invocation and response;
- commit serialization applies one configured total-order domain;
- eventual behavior is expressed by the absence of a causal, session, or
  linearizable freshness requirement.

The first matching policy rule is an alternative to later rules. Within the
selected behavior every consistency constraint applies. A fixed baseline that
is older than a later causal/session lower bound is unsatisfiable; the backend
MUST reject it rather than advance the baseline or weaken the lower bound.
Statically impossible combinations MUST prevent startup. Provider capability
mismatches MUST also prevent startup once a provider is selected.

When configured, the controller reads and writes opaque causal tokens in
`meta`. A provider joins multiple tokens using its frontier representation.
Clients store and forward tokens but never parse them. The CRUD business `data`
schema does not contain causal fields.

The SQLite authority represents a causal token by a published change frontier
and persists configured session frontiers across daemon restarts. Causal/session
behavior is restricted to immediate publication in version 1; unsupported
provider sources or frontier operations prevent daemon startup.

## 7. Artifact transfer

The `artifacts` capability enables chunked transfer of bytes owned by the
Patchouli backend. It does not change generic entity identity or Knowledge
references.

- `patchouli.artifact.upload.begin@1` validates upload metadata, allocates an
  opaque upload ID, and reports `max_chunk_bytes`.
- `patchouli.artifact.upload.chunk@1` accepts non-empty Base64 chunks in strict
  offset order. The server returns the next accepted byte offset.
- `patchouli.artifact.upload.commit@1` verifies the optional expected byte
  length and SHA-256 digest, publishes the bytes into managed storage, and
  creates the `artifact` entity through the configured backend controller.
- `patchouli.artifact.download.chunk@1` reads an active Artifact entity within
  the scope derived from request `meta`, then returns bytes from the selected
  version starting at the requested offset.

The server MUST NOT expose its filesystem paths. A managed placement key is an
opaque storage identifier to clients; the shipped local store uses a SHA-256
content address internally. Equal content MAY share one stored object. Deleting
or updating an Artifact entity MUST NOT immediately delete shared bytes.

Upload chunks MUST contain at most the negotiated limit. The commit request's
`meta` selects the Artifact scope and backend policy; begin and chunk metadata
do not make incomplete bytes visible. Upload commit returns the ordinary
mutation result, including its entity version and configured response metadata.
An upload that has not committed is not an entity and is not visible to reads
or subscriptions.

Download MUST resolve the Artifact entity before reading bytes, so possession
of a managed key cannot bypass configured scope. When no version is requested,
the entity MUST have exactly one active head. A requested version MUST be an
active version in the scoped read result. Indexed placements and managed
placements owned by another node return `UNSUPPORTED_CAPABILITY`; automatic
node routing and blob replication are outside version 1.

A client downloading without a requested version MUST pin the version returned
by the first chunk on every subsequent chunk request. If that version ceases to
be an active head before completion, the download fails instead of combining
bytes from different Artifact versions.

The shipped daemon discards incomplete uploads at restart. A content object may
remain unreferenced if byte publication succeeds but entity creation never
succeeds; safe garbage collection is outside version 1.

## 8. Entity retrieval

`patchouli.entity.retrieve@1` searches active published entities inside the
scope selected from request `meta`. Its business data is `query`, optional
`types`, and an optional `limit` from 1 through 100. Results contain scored
hits with the current entity variants. The RPC remains entity-generic;
deployments choose which types (for example `knowledge`) a consumer requests.
The initial SQLite provider performs case-insensitive lexical matching over the
stored JSON value. Retrieval never exposes staged work-unit candidates.

## 9. Change subscriptions

Change delivery is committed, ordered, resumable, and at least once.

- With no `after_cursor`, subscribe starts at the current end of the change log.
  The result cursor is that boundary; only later changes are sent.
- With `after_cursor`, the result cursor is the accepted replay position. Events
  begin strictly after it.
- The server MUST send the subscribe response before the first notification for
  that subscription.
- Cursors increase in server-defined order, but clients MUST treat them as
  opaque. Notifications on one subscription follow cursor order.
- Physical provider routing is deployment state outside this wire schema. One
  scope MUST remain pinned to one provider authority and cursor domain for the
  lifetime of its cursors, causal tokens, sessions, idempotency identities and
  open work units. Route changes require explicit data movement; a failed
  request MUST NOT fall back to another provider.
- Consumers MUST apply idempotently, persist the last applied cursor, and dedupe
  repeated cursors. A Harness binding may remain policy-stateless by forwarding
  the cursor to its consumer; the backend does not silently acknowledge delivery.
- Only published mutations produce events. A rolled-back acceptance or failed
  publication produces none. Changes carry configured mutation metadata in
  event `meta`; one configured batch may share a causal token, and its
  configured close-marker change is
  ordered last.
- If a requested cursor is older than retained history, subscribe returns
  `CURSOR_EXPIRED`. Retention is at least `change_retention_seconds`.
- A subscription belongs to one connection. Disconnect removes it but does not
  invalidate its last cursor.
- Unsubscribe returns `removed: false` when the ID is already absent.

`kind` is derived from the transition: `created` for a new identity,
`conflicted` when multiple heads become visible, `resolved` when multiple heads
become one, `deleted` for an ordinary tombstone transition, and `updated` for an
ordinary active transition.

## 10. Errors

JSON-RPC standard errors remain available. Patchouli domain errors use:

- `-32602 INVALID_REQUEST` for metadata or entity values that fail configured
  validation. Its `error.data.reason` is `INVALID_REQUEST`.

| Code | Reason | Meaning |
| ---: | --- | --- |
| -32001 | `UNAUTHENTICATED` | Authentication is absent or invalid. |
| -32002 | `FORBIDDEN` | Scope or operation is not permitted. |
| -32003 | `NOT_FOUND` | The entity identity does not exist. |
| -32004 | `VERSION_CONFLICT` | Configured base versions differ from current heads. |
| -32005 | `IDEMPOTENCY_CONFLICT` | A key was reused for another mutation. |
| -32006 | `UNSUPPORTED_CAPABILITY` | Version, consistency, or capability is unavailable. |
| -32007 | `DEADLINE_EXCEEDED` | Deadline elapsed before completion. |
| -32009 | `OVERLOADED` | Server cannot currently accept the request. |
| -32010 | `CURSOR_EXPIRED` | Replay position is outside retained history. |
| -32011 | `WORK_UNIT_EXPIRED` | Configured cross-request state expired before publication. |

`error.data.reason` MUST match the code. `VERSION_CONFLICT` MUST return either
`current_versions` for one request entity or `conflicts` containing every
conflicting entity reference and its current versions for grouped publication.
Error message text is diagnostic and MUST NOT drive client logic.

## 11. Complete wire examples

Handshake:

```json
{"jsonrpc":"2.0","id":1,"method":"patchouli.protocol.handshake@1","params":{"client":{"name":"dsh-patchouli","version":"0.1.0","instance_id":"client-7"},"protocol_versions":[1],"capabilities":["artifacts","subscriptions"]}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"protocol_version":1,"server":{"version":"0.1.0","cluster_id":"cluster-a","node_id":"node-1"},"capabilities":["artifacts","subscriptions"],"limits":{"max_request_bytes":1048576,"max_artifact_chunk_bytes":524288,"max_result_items":100,"idempotency_retention_seconds":86400,"change_retention_seconds":604800}}}
```

Create:

```json
{"jsonrpc":"2.0","id":2,"method":"patchouli.entity.create@1","params":{"meta":{"workspace":"alpha","channel_id":"channel-7","transaction_id":"tx-1","idempotency_key":"create-42"},"data":{"type":"note","id":"note-42","value":{"text":"hello"}}}}
```

```json
{"jsonrpc":"2.0","id":2,"result":{"meta":{"causal_token":"c1"},"data":{"entity":{"ref":{"type":"note","id":"note-42"},"version":"v1","state":"active","value":{"text":"hello"}}}}}
```

Read:

```json
{"jsonrpc":"2.0","id":3,"method":"patchouli.entity.read@1","params":{"meta":{"workspace":"alpha","channel_id":"channel-7","transaction_id":"tx-1","causal_token":"c1"},"data":{"ref":{"type":"note","id":"note-42"}}}}
```

```json
{"jsonrpc":"2.0","id":3,"result":{"meta":{"causal_token":"c1"},"data":{"state":"active","variants":[{"ref":{"type":"note","id":"note-42"},"version":"v1","state":"active","value":{"text":"hello"}}]}}}
```

Update:

```json
{"jsonrpc":"2.0","id":4,"method":"patchouli.entity.update@1","params":{"meta":{"workspace":"alpha","channel_id":"channel-7","transaction_id":"tx-1","idempotency_key":"update-42","base_versions":["v1"],"causal_token":"c1","conflict_strategy":"merge"},"data":{"ref":{"type":"note","id":"note-42"},"value":{"text":"hello again"}}}}
```

```json
{"jsonrpc":"2.0","id":4,"result":{"meta":{"causal_token":"c2"},"data":{"entity":{"ref":{"type":"note","id":"note-42"},"version":"v2","state":"active","value":{"text":"hello again"}}}}}
```

Delete:

```json
{"jsonrpc":"2.0","id":5,"method":"patchouli.entity.delete@1","params":{"meta":{"workspace":"alpha","channel_id":"channel-7","transaction_id":"tx-1","idempotency_key":"delete-42","base_versions":["v2"],"causal_token":"c2"},"data":{"ref":{"type":"note","id":"note-42"}}}}
```

```json
{"jsonrpc":"2.0","id":5,"result":{"meta":{"causal_token":"c3"},"data":{"entity":{"ref":{"type":"note","id":"note-42"},"version":"v3","state":"deleted"}}}}
```

Subscribe and event:

```json
{"jsonrpc":"2.0","id":6,"method":"patchouli.changes.subscribe@1","params":{"meta":{"workspace":"alpha","channel_id":"channel-7"},"data":{"filter":{"types":["note"]},"after_cursor":"cursor-8"}}}
```

```json
{"jsonrpc":"2.0","id":6,"result":{"meta":{},"data":{"subscription_id":"sub-1","cursor":"cursor-8"}}}
```

```json
{"jsonrpc":"2.0","method":"patchouli.changes.event@1","params":{"meta":{"causal_token":"c3","transaction_id":"tx-1"},"data":{"subscription_id":"sub-1","change":{"cursor":"cursor-9","ref":{"type":"note","id":"note-42"},"kind":"deleted","head_versions":["v3"]}}}}
```

Unsubscribe:

```json
{"jsonrpc":"2.0","id":7,"method":"patchouli.changes.unsubscribe@1","params":{"meta":{},"data":{"subscription_id":"sub-1"}}}
```

```json
{"jsonrpc":"2.0","id":7,"result":{"meta":{},"data":{"removed":true}}}
```

Version conflict:

```json
{"jsonrpc":"2.0","id":4,"error":{"code":-32004,"message":"The supplied base versions do not match the current heads.","data":{"reason":"VERSION_CONFLICT","current_versions":["v2"]}}}
```
