# Backend configuration

Database policy is deployment configuration, not part of the CRUD wire schema.
Except for the handshake, protocol messages use `{ meta, data }`. `data`
contains method business fields; the backend interprets `meta` through named
configuration fields.

This policy file does not select or configure a physical database provider.
Provider connection settings belong to daemon startup; for the initial SQLite
adapter that setting is the database file path.

The frontend plugin does not select consistency or maintain transaction state.
It may request a conflict strategy through the configured metadata field; when
that field is absent, the selected backend behavior supplies the default.
The request path is:

```text
JSON-RPC adapter -> backend engine -> policy selector -> database provider
```

The normative configuration shape is
[`config/patchouli.schema.json`](../config/patchouli.schema.json). The default
single-node SQLite policy is
[`config/patchouli.default.json`](../config/patchouli.default.json). The more
advanced shared-transaction example is
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

The policy selector validates every present bound value before it matches a
rule or derives a key. An invalid value fails the request; it is never treated
as an absent field to select a weaker fallback behavior.

Channel, transaction, timestamp, routed-plugin, request, causal, and
base-version values are ordinary configured metadata. Renaming one changes only
the deployment configuration and frontend metadata mapping, not CRUD `data` or
the JSON-RPC method family.

## Key roles

The configuration deliberately separates identities by engine role:

- `entity_identity.scope_by` optionally namespaces every entity and change
  record. The storage identity is `configured scope + (type, id)`.
- shared `consistency.snapshot.key_by` identifies requests that share one
  fixed database baseline.
- session `consistency.sessions[].key_by` identifies persisted monotonic-read
  and read-your-writes progress.
- acquisition and commit ordering keys identify their linearization domains.
- keyed `idempotency.key_by` identifies one logical mutation for retry
  deduplication.
- batch `publication.key_by` identifies candidates published together.

Every durable control key is `configured scope + key_by`. The scope is an
authorization and isolation prefix; `key_by` supplies only the role-specific
suffix. A `key_by` must not repeat a field already used by `scope_by`. A plugin
participant ID can therefore identify a mutation without
entering the entity storage key. Concurrent plugins still address the same
entity and enter the configured conflict policy.

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

Every behavior maps directly to an engine phase. Consistency is a conjunction
of phase-specific constraints rather than one level enum:

- `consistency.snapshot.mode: request` chooses one snapshot per request and
  stores no shared baseline.
- `consistency.snapshot.mode: shared` fixes a baseline by `key_by`. It is valid
  only with batch publication using the same key fields.
- `consistency.acquire.allow_sources` defines the initial authority/replica
  candidate set.
- `causal_after` contributes an opaque causal-token lower bound. Multiple
  bounds are joined by the provider.
- `linearizable` intersects the source set with authority and identifies one
  linearization domain.
- `consistency.sessions` adds monotonic-read and read-your-writes lower bounds
  keyed by configured identities.
- `consistency.commit.ordering` either adds no ordering or serializes commits
  in one configured domain.
- `idempotency.mode: disabled` creates no idempotency record or key.
- `idempotency.mode: keyed` requires the complete `key_by` for a mutation and
  its stored result.
- `conflict.default_strategy` selects `merge`, `mvcc`, or `reject` when the
  request does not override it.
- `conflict.strategy_from` identifies the optional metadata field containing a
  request override.
- `conflict.base_versions` identifies the metadata field containing the
  candidate's base version set.
- `conflict.merge` declares JSON Pointer fields reconciled with Automerge and
  the relative discriminator fields that must match before merging.
- `conflict.otherwise` chooses `mvcc` or `reject` outside a merge rule.
- `publication.mode` is explicitly `immediate` or `batch`.

A batch publication additionally declares its `key_by`, exactly one
`close_when` condition, a positive `staging_ttl_ms`, and `on_expire` behavior.
Close conditions are marker equality, expected member count, or an event-time
window.

Configuration rules are alternatives: the first matching rule selects one
behavior. Constraints inside that behavior combine with logical AND. Source
sets are intersected, causal/session lower bounds are joined, and exact shared
baselines cannot move forward. An impossible combination rejects configuration
at startup when static, or rejects the request when it depends on persisted
frontiers. The engine never weakens a guarantee to find a fallback execution.

Each session identity is declared once. Put all guarantees for the same
`key_by` in that declaration; two declarations with the same effective key are
rejected instead of creating two writers for one persisted session frontier.

## Common consistency patterns

All patterns below are complete configuration files validated by the same
Schema and semantic checker. They compile into execution plans; a selected
provider must still advertise the sources and frontier operations required by
the plan.

### Default single-node SQLite

[`config/patchouli.default.json`](../config/patchouli.default.json) is the
shipped default:

- one request snapshot acquired from the authority;
- linearizable acquisition within the configured channel scope;
- commits serialized within that same scope;
- immediate publication;
- `merge` by default for Knowledge: Automerge at `/content`, grouped by
  `/content/kind`, with MVCC for every other field;
- `mvcc` by default for KnowledgeRelation;
- no session, batch, replica, or idempotency state.

Both ordering declarations use an empty `key_by` because `channel_id` is
already supplied as the implicit scope prefix. The resulting domain is the
channel, not a process-global lock.

### Eventual multi-source reads

[`config/patterns/eventual.json`](../config/patterns/eventual.json)
allows authority or replica snapshots, adds no freshness/session constraint,
and adds no commit ordering. It uses `mvcc`, so independently accepted
concurrent candidates remain visible.

### Causal plugin session

[`config/patterns/causal_session.json`](../config/patterns/causal_session.json)
accepts authority or replica snapshots, joins an optional opaque causal token
with persisted `monotonic_reads` and `read_your_writes` progress for one routed
plugin, and serializes commits within scope. The participant field is required;
the causal token is optional, and its absence contributes no lower bound.

### Shared transaction batch

[`config/patterns/shared_transaction.json`](../config/patterns/shared_transaction.json)
requires transaction, participant, and request identities for its selected
behavior. The shared-snapshot and publication keys are identical, each plugin
gets scoped session and idempotency identities, and a commit marker publishes
the group. [`config/patchouli.example.json`](../config/patchouli.example.json)
shows how to select this pattern by metadata presence and retain the
default-style linearizable fallback for other requests.

## Transaction boundary

Every CRUD mutation uses one short database transaction. It atomically records
the immutable candidate or tombstone, enabled idempotency and causal/group
state, and—when publication is immediate—the visible head and change record.

Batch mode persists candidates and group state instead of keeping a database
transaction open between RPC calls. The closing mutation publishes all accepted
candidates and their change records in one short transaction. A successful
mutation response means durable acceptance; the change stream reports
publication.

## Conflict resolution

The configured `strategy_from` alias keeps the physical request key
deployment-specific. The shipped configurations bind it to
`meta.conflict_strategy`. Omitting it uses `default_strategy`; supplying it
overrides only conflict handling and does not change consistency, idempotency,
scope, or publication.

The default Knowledge merge rule is:

```json
{
  "path": "/content",
  "strategy": "automerge",
  "group_by": ["/kind"]
}
```

Clients continue to send complete JSON replacements. The backend diffs each
replacement against its declared base before merging concurrent branches.
Strings use collaborative text; maps and lists use Automerge objects and
sequences. Lists are reconciled positionally in version 1; applications that
need identity-aware list moves should model elements as a keyed map.

When `base_versions` contains several heads, the backend first joins their
stored Automerge frontiers and creates the candidate change on that combined
frontier. The candidate therefore records every selected CRDT head as a parent
without requiring a new RPC shape.

The discriminator prevents text and structured content from being forced into
one CRDT representation. With `otherwise: mvcc`, those values remain separate
heads. All fields outside `/content` also remain MVCC. If two candidates merge
to the same content but have different metadata, the backend retains two
derived versions with the shared merged content. Identical resulting versions
collapse to one.

SQLite persists immutable materialized entity JSON together with Automerge
changes, dependency edges, and the change frontier attached to each entity
version and field path. Old versions remain readable; merging creates derived
versions rather than mutating their materialized JSON.
