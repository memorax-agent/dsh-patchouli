# Patchouli Provider Contract

Shared lifecycle boundary between the backend daemon and a concrete database
adapter. The daemon receives a `Provider` and does not depend on its connection
type.

Each provider advertises authority/replica, retrieval, change-stream,
idempotency, work-unit, causal-read, monotonic-read, read-your-writes, and
linearizable-read capabilities before initialization.
The engine validates every configured behavior against that descriptor and
refuses startup when no valid execution path exists.

The contract owns the durable process lifecycle, published entity snapshots,
atomic compare-and-swap commits, consistency-aware reads, and durable
cross-request work units. A read selects an allowed source, validates every
causal and session lower bound, reads one snapshot, and advances configured
monotonic-read frontiers in one provider operation. Read-your-writes frontiers
advance only with successful publication.

A work unit records its canonical scope, one provider-local published cursor
and immutable policy descriptor when it opens, reconstructs every entity baseline at that cursor, persists staged
versions, heads, and effective conflict policies between RPC calls, and
atomically publishes every mutated entity on marker close. On baseline drift it
returns complete entity snapshots to the engine and accepts one resolved atomic
publication guarded by the latest published heads. Immediate and grouped commits both keep immutable
versions, CRDT changes/frontiers, visible heads, and change records within their
required SQLite transaction boundary. Change records carry configured mutation
metadata, and session write frontiers advance atomically with publication. Change
waiting is provider-backed, so subscriptions observe commits made through a
remote storage node rather than relying on an engine-process notification.
