# Patchouli Server

Cross-platform daemon shell and control CLI for Patchouli. It owns process
lifecycle, local IPC, JSON-RPC handshake, status, graceful shutdown, and
composition of `BackendEngine` with scope-routed local and remote providers. CRUD, generic
retrieval, and resumable change subscriptions
are routed to the engine and use transactional storage under the shipped
single-node policy.

Local transports:

- macOS/Linux: Unix domain socket;
- Windows: named pipe;
- framing: UTF-8 NDJSON, one JSON-RPC object per line.

```bash
cargo install --path crates/server
patchouli-db init --root "$HOME/.patchouli"
patchouli-db serve --endpoint <endpoint> --artifacts <artifact-root> --providers <provider-config> --config <policy-path>
patchouli-db provide --listen 127.0.0.1:8080 --database <path> --token-env PATCHOULI_PROVIDER_TOKEN --change-retention-seconds 604800
patchouli-db status --endpoint <endpoint>
patchouli-db checkpoint --endpoint <endpoint>
patchouli-db stop --endpoint <endpoint>
patchouli-db config check config/patchouli.default.json --providers config/providers.local.json
```

Release installation and supported architectures are documented in
[`docs/installation.md`](../../docs/installation.md). `init` creates the data
and run directories plus validated default policy/provider files, and never
overwrites an existing file.

The daemon also owns a scoped, content-addressed Artifact store. Chunked upload
commit creates the managed `artifact` entity through `BackendEngine`; chunked
download resolves that entity before reading bytes, so a content key cannot
bypass configured scope. Indexed Artifacts remain external to this store.

The server library accepts the provider contract rather than a SQLite
connection. The shipped CLI always names its local SQLite provider `local`, may
connect named remote storage nodes, and reports `routed` in the control status
response. Scope rules select exactly one provider and never fail over. SQLite is the default Cargo feature; consumers
embedding only the server library may disable default features and inject
another provider.

Handshake capabilities are negotiated by intersection. The reusable local
client queues change notifications while ordinary responses are outstanding
and exposes subscribe/unsubscribe operations without depending on Harness.

`status` reports the provider generation and whether startup followed an
unclean shutdown. On `stop`, the daemon stops accepting clients, signals and
drains connection tasks, then shuts down the engine and provider.

`patchouli-db provide` owns one SQLite authority, including its change-log
retention policy, and exposes provider primitives.
Read its bearer token only from the named environment variable. Bind it to
loopback behind an HTTPS reverse proxy for remote access; non-loopback remote
clients reject cleartext HTTP.
