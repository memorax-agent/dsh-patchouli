# Patchouli Provider Contract

Shared lifecycle boundary between the backend daemon and a concrete database
adapter. The daemon receives a `Provider` and does not depend on its connection
type.

The contract owns the durable process lifecycle: initialize and report recovery
state, perform health checks, checkpoint committed state, and shut down cleanly.
CRUD and transaction primitives will be added with the backend engine so their
shape is driven by real engine operations.
