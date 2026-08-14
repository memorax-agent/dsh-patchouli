# Backend configuration

Database policy is deployment configuration, not part of the CRUD wire schema.
Except for the handshake, protocol messages use `{ meta, data }`. `data`
contains only method business fields; `meta` is open JSON interpreted by the
backend for entity types configured locally.

The frontend plugin is stateless with respect to this policy. The request path
is:

```text
JSON-RPC adapter -> Rust backend controller -> policy engine -> database provider
```

The controller, not the frontend or provider, owns validation, rule selection,
logical baselines, durable group state, closing timers, conflicts, publication,
and construction of the protocol response. The provider only supplies storage,
snapshot, transaction, and change-log primitives.

The example configuration is [`config/patchouli.example.json`](../config/patchouli.example.json).

## Normalized operation document

Named fields use RFC 6901 JSON Pointers over the received `{ meta, data }`
document:

```json
{
  "meta": {
    "channel_id": "channel-7",
    "transaction_id": "transaction-3",
    "event_time": "2026-08-14T08:00:00Z",
    "plugin_route_id": "route-a",
    "causal_token": "causal-2",
    "idempotency_key": "request-9",
    "base_versions": ["version-1"]
  },
  "data": {
    "type": "event",
    "id": "event-42",
    "value": { "payload": {} }
  }
}
```

For create and update, `data.value` is the proposed entity value. Read and
delete contain only `data.ref`. A required selector that cannot be resolved
makes the operation invalid. Identity and consistency rules refer to field
aliases, never directly to hard-coded metadata names.

## Schema and identity

Each configured entity type contains an inline JSON Schema for `data.value`.
The controller validates a proposed value before persistence. Every selected
consistency behavior has its own ordered `identity` list, so a mixed model may
use different identities without altering the wire-level `EntityRef`.

Channel, transaction, timestamp, routed-plugin, causal, idempotency, and
base-version values are therefore ordinary configured metadata fields. Another
deployment may choose different names and pointers without changing CRUD
`data` or an RPC method.

## Mixed consistency rules

Rules are evaluated in file order. The first rule whose
`when_all_present` selectors resolve is applied; otherwise `fallback` is used.

- `group_by` identifies operations sharing one logical baseline or batch.
- `baseline.consistency` chooses eventual, causal, or linearizable acquisition.
- `baseline.source` chooses any node, an authority, or a replica.
- `baseline.at_field` optionally anchors the baseline to a configured logical
  time field. The actual database snapshot token remains server-owned and
  opaque.
- `baseline.causal_field` optionally names the configured metadata field that
  carries opaque causal progress.
- `conflict.strategy: reject` rejects a conflicting publication.
- `conflict.strategy: preserve_heads` retains concurrent candidates for
  explicit later resolution.
- `conflict.base_versions_field` optionally names the configured metadata field
  containing the candidate's base versions.

Consistency rules cannot infer when an open batch is complete. A batch must
declare exactly one close condition:

- `marker`: a configured field equals a configured JSON value;
- `expected_count`: the configured field gives the expected member count;
- `time_window`: event time crosses a fixed window plus allowed lateness.

`staging_ttl_ms` bounds abandoned controller state. `on_expire` explicitly
chooses whether an incomplete batch is discarded or published under its normal
conflict policy. The controller persists group membership, selected baseline,
candidates, close state, and conflicts; the frontend does not reconstruct them
after reconnecting.

## Durability and visibility

Every CRUD mutation is atomically appended to the durable operation journal
together with its idempotency result. Without a batch, the same database
transaction also publishes the entity head and change record.

With `visibility: on_close`, individual mutations are durable candidates but
are not published as visible heads or subscription changes until the close
condition fires. Closing a batch publishes all accepted candidates and their
change records in one short database transaction. The backend never keeps a
database connection transaction open between RPC calls.

A successful mutation response means durable acceptance. Configuration decides
whether publication is immediate or deferred. Consumers that use deferred
profiles must treat the change stream or the configured batch-control entity as
the publication result.
