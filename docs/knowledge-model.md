# Knowledge fact model

Patchouli fact IR version 1 has exactly two public record values:

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
                                  +-- KnowledgeValue or KnowledgeRelationValue
```

The generic entity envelope is the only authority for an entity ID and storage
version. Fact values therefore do not repeat `id` or `revision` inside
`metadata`. This prevents two identities or revisions from disagreeing.

The configured request `meta` produces `scope_json`, which is part of every
database key and is the authorization/storage boundary. `metadata.core.scope`
describes the fact's semantic tenant/workspace/user/session context; it is
untrusted input and cannot widen the configured storage scope.

## Knowledge

Every `KnowledgeValue` has four required fields:

```text
content   text or structured JSON
metadata  fixed core plus namespaced extensions
artifact  zero or more immutable references
profile   seven behavior dimensions
```

`content` is either `{ kind: "text", text }` or
`{ kind: "structured", value }`. Binary data and vectors are never embedded in
content.

`metadata.core` fixes schema identity, semantic scope, source origin, timestamps,
lifecycle, and provenance. All nullable core fields remain present as `null`,
so omission is not confused with an unknown value. Extension keys contain at
least one namespace separator, for example `local.session`.

Artifacts have the roles `source`, `attachment`, or `embedding`. Every artifact
is addressed by `ref` and protected by `digest`; locators must not contain
credentials. Embeddings additionally record model, dimensions, metric, and the
opaque entity `source_version`. An embedding derived from another knowledge
version is stale and must not be attached as current materialization.

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

SQLite storage schema version 3 defines two authority tables:

- `patchouli_entity_version`: immutable active values and tombstones keyed by
  canonical `scope_json + entity_type + entity_id + version`;
- `patchouli_entity_head`: the currently published head set, allowing one head
  normally and multiple heads under the `mvcc` conflict strategy.

It also defines `patchouli_crdt_change`, `patchouli_crdt_change_parent`, and
`patchouli_entity_crdt_head` for Automerge changes, their dependency graph, and
the per-version field frontier.

Active rows require valid JSON; tombstones require a null value. The value is
stored once. `patchouli_knowledge` and `patchouli_knowledge_relation` are read-only
views over active published heads. They expose typed columns such as knowledge
content/profile and relation type plus complete `from`/`to` reference arrays
without becoming a second semantic authority.

Earlier storage schemas are obsolete and rejected explicitly; no migration or
compatibility path is maintained at this stage.
