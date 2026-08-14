# Patchouli Provider Contract

Shared lifecycle boundary between the backend daemon and a concrete database
adapter. The daemon receives a `Provider` and does not depend on its connection
type.

The contract currently covers provider identity and startup health only. CRUD
and transaction primitives will be added with the backend engine so their shape
is driven by real engine operations.
