# Patchouli Server

Cross-platform daemon shell and control CLI for Patchouli. It owns process
lifecycle, local IPC, JSON-RPC handshake, status, graceful shutdown, and
composition of `BackendEngine` with the default SQLite provider. CRUD requests
are routed to the engine, whose storage behavior is currently a placeholder.

Local transports:

- macOS/Linux: Unix domain socket;
- Windows: named pipe;
- framing: UTF-8 NDJSON, one JSON-RPC object per line.

```bash
cargo install --path crates/server
patchouli serve --endpoint <endpoint> --database <path> --config <policy-path>
patchouli status --endpoint <endpoint>
patchouli stop --endpoint <endpoint>
patchouli config check config/patchouli.example.json
```

The server library accepts the provider contract rather than a SQLite
connection. The shipped CLI selects the SQLite adapter and reports `sqlite` in
the control status response. SQLite is the default Cargo feature; consumers
embedding only the server library may disable default features and inject
another provider.
