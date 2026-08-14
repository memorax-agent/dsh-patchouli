# Patchouli Remote Provider

Transports Patchouli provider primitives to a storage node over authenticated
HTTP. Non-loopback clients require HTTPS; a storage node can bind HTTP on
loopback behind a TLS reverse proxy. Bearer tokens are read from environment
variables and never stored in provider configuration.

The remote endpoint exposes database primitives, not public CRUD JSON-RPC, so
the calling backend engine remains the only owner of schemas, consistency,
conflict and publication logic. Normal calls have a 30-second deadline;
provider-backed change waiting uses a long poll that ends when its connection closes.
