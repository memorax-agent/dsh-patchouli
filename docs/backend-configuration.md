# Backend configuration

Database policy is deployment configuration, not part of the CRUD wire schema.
Except for the handshake, protocol messages use `{ meta, data }`. `data`
contains method business fields; the backend interprets `meta` through named
configuration fields.

This policy file does not select or configure a physical database provider.
Provider connection settings belong to daemon startup; for the initial SQLite
adapter that setting is the database file path.

The frontend plugin does not select consistency or maintain transaction state.
The request path is:

```text
JSON-RPC adapter -> backend engine -> policy selector -> database provider
```

The normative configuration shape is
[`config/patchouli.schema.json`](../config/patchouli.schema.json). A complete
deployment example is
[`config/patchouli.example.json`](../config/patchouli.example.json).

The default configuration registers the built-in fact schemas by stable URN:

```json
"knowledge": {
  "value_schema": { "$ref": "urn:patchouli:schema:knowledge:1" }
}
```

The backend resolves the built-in Knowledge and KnowledgeRelation schemas from the
installed package without network access. Inline deployment schemas remain
valid for other entity types.

## Validation

Configuration is checked in two layers at backend startup:

1. JSON Schema rejects missing properties, invalid enums, invalid shapes, and
   unknown properties.
2. Rust semantic validation checks JSON Pointer syntax, embedded JSON Schemas,
   duplicate names, and every metadata alias referenced by a rule.

Invalid configuration prevents the backend from starting. There is no runtime
fallback to a partially valid policy.

## Metadata fields

`meta_fields` gives deployment-specific metadata stable aliases. Each pointer
is relative to the request `meta` object, not to the full JSON-RPC envelope.
Each field also contains a JSON Schema for its runtime value.

```json
{
  "channel_id": {
    "pointer": "/channel_id",
    "schema": { "type": "string", "minLength": 1 }
  },
  "causal_token": {
    "pointer": "/causal_token",
    "schema": { "type": "string", "minLength": 1 }
  }
}
```

Channel, transaction, timestamp, routed-plugin, request, causal, and
base-version values are ordinary configured metadata. Renaming one changes only
the deployment configuration and frontend metadata mapping, not CRUD `data` or
the JSON-RPC method family.

## Key roles

The configuration deliberately separates identities by engine role:

- `entity_identity.scope_by` optionally namespaces every entity and change
  record. The storage identity is `configured scope + (type, id)`.
- shared `baseline.key_by` identifies requests that share database baseline
  state.
- keyed `idempotency.key_by` identifies one logical mutation for retry
  deduplication.
- batch `publication.key_by` identifies candidates published together.

Every `key_by` is complete and used exactly as written; the engine never adds
the entity scope implicitly. A plugin participant ID can therefore identify a
mutation without entering the entity storage key. Concurrent plugins still
address the same entity and enter the configured conflict policy.

## Entity policies and rules

Each entity type declares:

- `value_schema`, applied to proposed `data.value` on create and update;
- ordered `rules` selected from metadata presence;
- one explicit `fallback` behavior.

Rules are evaluated in file order. The first rule whose `when.all_present`
aliases are present is selected. A selected behavior references fields by alias;
missing fields required by the operation produce an error rather than selecting
another policy.

## Behavior

Every behavior maps directly to an engine phase:

- `baseline.mode: request` creates no persistent baseline key.
- `baseline.mode: shared` requires the complete `key_by` used for persistent
  baseline state.
- `baseline.consistency` selects eventual, causal, or linearizable reads.
- `baseline.source` selects any node, an authority, or a replica.
- `baseline.at` optionally anchors a new baseline to configured logical time.
- `baseline.causal_token` identifies the opaque token field read from request
  `meta` and written to result/event `meta`.
- `idempotency.mode: disabled` creates no idempotency record or key.
- `idempotency.mode: keyed` requires the complete `key_by` for a mutation and
  its stored result.
- `conflict.strategy` selects rejection or preservation of concurrent heads.
- `conflict.base_versions` identifies the metadata field containing the
  candidate's base version set.
- `publication.mode` is explicitly `immediate` or `batch`.

A batch publication additionally declares its `key_by`, exactly one
`close_when` condition, a positive `staging_ttl_ms`, and `on_expire` behavior.
Close conditions are marker equality, expected member count, or an event-time
window.

## Transaction boundary

Every CRUD mutation uses one short database transaction. It atomically records
the immutable candidate or tombstone, enabled idempotency and causal/group
state, and—when publication is immediate—the visible head and change record.

Batch mode persists candidates and group state instead of keeping a database
transaction open between RPC calls. The closing mutation publishes all accepted
candidates and their change records in one short transaction. A successful
mutation response means durable acceptance; the change stream reports
publication.
