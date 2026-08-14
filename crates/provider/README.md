# Patchouli Provider Contract

Shared lifecycle boundary between the backend daemon and a concrete database
adapter. The daemon receives a `Provider` and does not depend on its connection
type.

Each provider advertises authority/replica, retrieval, change-stream,
idempotency, work-unit, and causal/session capabilities before initialization.
The engine validates every configured behavior against that descriptor and
refuses startup when no valid execution path exists.

The contract owns the durable process lifecycle, published entity snapshots,
atomic compare-and-swap commits, and durable cross-request work units. A work
unit records one global published cursor and immutable policy descriptor when
it opens, reconstructs every entity baseline at that cursor, persists staged
versions, heads, and effective conflict policies between RPC calls, and
atomically publishes every mutated entity on marker close. On baseline drift it
returns complete entity snapshots to the engine and accepts one resolved atomic
publication guarded by the latest published heads. Immediate and grouped commits both keep immutable
versions, CRDT changes/frontiers, visible heads, and change records within their
required SQLite transaction boundary. Change records carry configured mutation
metadata, and session frontiers advance atomically with publication.
