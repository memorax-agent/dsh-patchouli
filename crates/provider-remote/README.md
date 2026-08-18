# Patchouli Remote Provider

Transports Patchouli provider primitives to a storage node over authenticated
HTTP. Non-loopback clients require HTTPS; a storage node can bind HTTP on
loopback behind a TLS reverse proxy. Bearer tokens are read from environment
variables and never stored in provider configuration.

The remote endpoint exposes database primitives, not public CRUD JSON-RPC, so
the calling backend engine remains the only owner of schemas, conflict and
publication policy. Compiled consistency constraints travel with the read so
the storage authority performs frontier validation and the read atomically.
Normal calls have a 30-second deadline;
provider-backed change waiting uses a long poll that ends on a change, connection
close, or storage-node shutdown.

The transport is a versioned private protocol between Patchouli nodes; the
current endpoint is `/provider/v2`. Change-log retention is enforced by the
storage authority; callers cannot choose its cleanup cutoff. Serialized
call/reply fixtures are tested in the crate; any wire-shape change must update
those fixtures and increment the protocol version before nodes are deployed.
