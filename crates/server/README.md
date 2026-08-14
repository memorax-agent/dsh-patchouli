# Patchouli Server

Cross-platform daemon shell and control CLI for Patchouli. It owns process
lifecycle, local IPC, JSON-RPC handshake, status, graceful shutdown, and
composition of `BackendEngine` with the default SQLite provider. CRUD, generic
retrieval, and resumable change subscriptions
are routed to the engine and use transactional storage under the shipped
single-node policy.

Local transports:

- macOS/Linux: Unix domain socket;
- Windows: named pipe;
- framing: UTF-8 NDJSON, one JSON-RPC object per line.

```bash
cargo install --path crates/server
patchouli serve --endpoint <endpoint> --database <path> --config <policy-path>
patchouli status --endpoint <endpoint>
patchouli checkpoint --endpoint <endpoint>
patchouli stop --endpoint <endpoint>
patchouli config check config/patchouli.default.json
```

The server library accepts the provider contract rather than a SQLite
connection. The shipped CLI selects the SQLite adapter and reports `sqlite` in
the control status response. SQLite is the default Cargo feature; consumers
embedding only the server library may disable default features and inject
another provider.

Handshake capabilities are negotiated by intersection. The reusable local
client queues change notifications while ordinary responses are outstanding
and exposes subscribe/unsubscribe operations without depending on Harness.

`status` reports the provider generation and whether startup followed an
unclean shutdown. On `stop`, the daemon stops accepting clients, signals and
drains connection tasks, then shuts down the engine and provider.
