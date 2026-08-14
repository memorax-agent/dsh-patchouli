# Patchouli SQLite Provider

Default local provider adapter. It creates the database parent directory, opens
one asynchronous SQLite connection, enables foreign keys and WAL mode, and
performs the health check required by the daemon startup boundary.

The crate uses a bundled SQLite build so the same adapter can be compiled on
macOS, Linux, and Windows without a system SQLite installation.
