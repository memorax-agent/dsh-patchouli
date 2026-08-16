# Patchouli SQLite Provider

Default local provider adapter. It creates the database parent directory, takes
an exclusive `<database>.lock`, opens one asynchronous SQLite connection, and
enables foreign keys and WAL mode. A runtime-state table records the storage
schema version, daemon generation, and clean/unclean shutdown state.

Startup lets SQLite recover committed WAL transactions before the daemon begins
listening. Manual checkpoints use complete WAL checkpointing. Graceful shutdown
marks the generation clean, truncates the WAL, closes the connection, and
releases the ownership lock.

Storage schema version 12 contains generic immutable entity versions, published
heads, non-expiring head history, a separately retained atomic change log,
durable causal/session frontiers, published and staged idempotency records,
Automerge change/frontier tables, and durable work units with one scope-local
baseline cursor, a fixed policy descriptor, per-entity
conflict policies, captured base versions, private staged heads, and a durable
sealed closing state. Marker
close either publishes unchanged baselines immediately or returns the complete
drift set for engine resolution; the resulting compare-and-swap publishes the
whole unit in one SQLite transaction. Expired open units are swept on startup,
checkpoint, shutdown, and every provider operation. Read-only
`patchouli_artifact`, `patchouli_knowledge`, and `patchouli_knowledge_relation`
views project typed fact fields from the one authoritative JSON value. The
views do not duplicate semantic state. A trigram FTS5 index accelerates literal
text retrieval across the authoritative JSON values, while scope, entity type,
and entity ID selection runs inside SQLite. SQLite advertises these capabilities
before engine startup so an incompatible backend policy cannot begin serving
requests.

The crate uses a bundled SQLite build so the same adapter can be compiled on
macOS, Linux, and Windows without a system SQLite installation.
