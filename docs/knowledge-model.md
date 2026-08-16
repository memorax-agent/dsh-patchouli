# Knowledge fact model

Patchouli fact IR version 1 has three public record values:

- `artifact`, validated by `urn:patchouli:schema:artifact:1`;
- `knowledge`, validated by `urn:patchouli:schema:knowledge:1`;
- `knowledge_relation`, validated by
  `urn:patchouli:schema:knowledge-relation:1`.

They use the existing generic entity CRUD methods. No knowledge-specific JSON-RPC
method exists. The canonical schemas and examples live under
[`packages/protocol/schemas`](../packages/protocol/schemas/).

## Identity

Entity identity and fact value are deliberately separate:

```text
configured storage scope + EntityRef(type, id) + opaque EntityVersion
                                  |
                                  +-- ArtifactValue, KnowledgeValue, or KnowledgeRelationValue
```

The generic entity envelope is the only authority for an entity ID and storage
version. Fact values therefore do not repeat `id` or `revision` inside
`metadata`. This prevents two identities or revisions from disagreeing.

The configured request `meta` produces `scope_json`, which is part of every
database key and is the authorization/storage boundary. `metadata.core.scope`
describes the fact's semantic tenant/workspace/user/session context; it is
untrusted input and cannot widen the configured storage scope.

## Artifact

Files, embeddings, and other non-JSON resources are first-class `artifact`
entities. Knowledge never embeds their bytes or repeats their storage details.
Every Artifact has media type, optional display name, optional byte length and
digest, semantic metadata, and exactly one placement:

```text
managed  Patchouli owns the bytes at provider + key
indexed  another provider owns the bytes at provider + locator + revision
```

Local paths and remote object identifiers use the same `indexed` shape. The
provider interprets its opaque locator; generic CRUD and Knowledge consumers do
not branch on local versus remote location. Locators and keys must not contain
credentials.

A managed Artifact requires both `byte_length` and `digest`, because the backend
cannot claim ownership without a complete content identity. An indexed Artifact
may leave either value null when the external provider exposes only a revision.
Changing the indexed revision or promoting an indexed Artifact to managed
storage creates a normal new entity version.

Managed bytes enter the backend through the Artifact upload RPCs. Upload commit
verifies their length and SHA-256 digest, stores equal content once, and creates
the Artifact entity through the normal configured scope, transaction,
consistency, conflict, and change-publication path. Downloads resolve that
scoped entity before reading bytes; clients never receive a backend filesystem
path. The placement provider is the owning daemon node ID, so another node can
reject the request without reading the wrong local store.

Indexed Artifacts do not use the managed upload/download RPCs. They are created
with generic CRUD, and the provider named by their placement interprets the
opaque locator. Deleting or superseding an Artifact entity does not immediately
delete managed bytes because content-addressed objects may be shared. Incomplete
uploads are discarded on daemon restart; orphan collection and cross-node byte
replication are not implemented in version 1.

Knowledge references an Artifact as `{ type: "artifact", id, role }`. The role
is contextual to that Knowledge (`source`, `attachment`, or `embedding`), while
media type, digest, placement, and provenance have one authority on the
Artifact entity.

## Knowledge

Every `KnowledgeValue` has four required fields:

```text
content   text or structured JSON
metadata  fixed core plus namespaced extensions
artifact  zero or more typed Artifact entity references
profile   seven behavior dimensions
```

`content` is either `{ kind: "text", text }` or
`{ kind: "structured", value }`. Binary data and vectors are never embedded in
content.

`metadata.core` fixes schema identity, semantic scope, source origin, timestamps,
lifecycle, and provenance. All nullable core fields remain present as `null`,
so omission is not confused with an unknown value. Extension keys contain at
least one namespace separator, for example `local.session`.

Artifact references have the roles `source`, `attachment`, or `embedding`.
Representation-specific metadata such as embedding model and dimensions belongs
to the Artifact metadata extensions rather than being copied into each
reference.

The profile dimensions are:

| Dimension | Values |
| --- | --- |
| epistemic | `unknown`, `observation`, `hypothesis`, `belief`, `knowledge`, `derived` |
| temporal | `unknown`, `timeless`, `instant`, `interval`, `sequence` |
| ownership | `unknown`, `world`, `agent`, `user`, `shared` |
| abstraction | `unknown`, `instance`, `pattern`, `concept`, `rule` |
| persistence | `unknown`, `working`, `short_term`, `long_term`, `permanent` |
| retrieval | one or more of `unknown`, `exact`, `associative`, `contextual`, `causal`, `procedural` |
| actionability | `unknown`, `informational`, `directive`, `constraint` |

Retrieval values describe cognitive behavior. Full-text, vector, trigram, and
other query implementations are not profile values. Actionability is also only
descriptive; a stored `directive` or `constraint` does not grant authority to
execute anything.

## KnowledgeRelation

A relation value contains a fixed relation type, a non-empty `from` collection,
a non-empty `to` collection, and the same metadata shape with its own schema
identity. References within each collection are unique. Version 1 relation
types are:

| Type | Direction |
| --- | --- |
| `supports` | supporting knowledge set → supported knowledge set |
| `contradicts` | contradicting knowledge set → contradicted knowledge set |
| `derived_from` | derived knowledge set → source knowledge set |
| `generalized_from` | generalized knowledge set → source instance set |
| `causes` | cause knowledge set → effect knowledge set |
| `supersedes` | replacement knowledge set → older knowledge set |

All endpoints are resolved in the relation entity's configured storage scope;
cross-scope relations are not part of version 1. Relations otherwise follow the
generic replacement semantics: an update may replace `type`, `from`, `to`, and
`metadata` together and creates a new opaque entity version. The two collections
may overlap, so self-relations and
cycles are valid records; the backend does not impose graph-topology rules.

JSON Schema checks the local record shape. When CRUD execution is implemented,
the controller will check endpoint existence, common scope, tombstones, and the
configured generic version/conflict policy, but it will not check cycles or
preserve an earlier relation's type or endpoints.

## SQLite entries

SQLite storage schema version 11 defines two authority tables:

- `patchouli_entity_version`: immutable active values and tombstones keyed by
  canonical `scope_json + entity_type + entity_id + version`;
- `patchouli_entity_head`: the currently published head set, allowing one head
  normally and multiple heads under the `mvcc` conflict strategy.

It also defines `patchouli_crdt_change`, `patchouli_crdt_change_parent`, and
`patchouli_entity_crdt_head` for Automerge changes, their dependency graph, and
the per-version field frontier. `patchouli_change` records every committed head
transition in the same transaction for later reactive delivery.

Each published version records the change cursor at which it became visible.
The `patchouli_work_unit*` tables use that cursor to reconstruct one fixed
database baseline across RPC calls, while keeping staged versions out of the
published heads and typed views until marker close.

Active rows require valid JSON; tombstones require a null value. The value is
stored once. `patchouli_artifact`, `patchouli_knowledge`, and
`patchouli_knowledge_relation` are read-only views over active published heads.
They expose typed columns such as Artifact placement, Knowledge content/profile,
and relation type plus complete `from`/`to` reference arrays without becoming a
second semantic authority.

Earlier storage schemas are obsolete and rejected explicitly; no migration or
compatibility path is maintained at this stage.
