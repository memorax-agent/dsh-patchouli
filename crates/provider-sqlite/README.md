# Patchouli SQLite Provider

Default local provider adapter. It creates the database parent directory, takes
an exclusive `<database>.lock`, opens one asynchronous SQLite connection, and
enables foreign keys and WAL mode. A runtime-state table records the storage
schema version, daemon generation, and clean/unclean shutdown state.

Startup lets SQLite recover committed WAL transactions before the daemon begins
listening. Manual checkpoints use complete WAL checkpointing. Graceful shutdown
marks the generation clean, truncates the WAL, closes the connection, and
releases the ownership lock.

The crate uses a bundled SQLite build so the same adapter can be compiled on
macOS, Linux, and Windows without a system SQLite installation.
