# Patchouli Server

Cross-platform daemon shell and control CLI for Patchouli. It currently owns
only process lifecycle, local IPC, JSON-RPC handshake, status, and graceful
shutdown. It does not implement CRUD or open a database yet.

Local transports:

- macOS/Linux: Unix domain socket;
- Windows: named pipe;
- framing: UTF-8 NDJSON, one JSON-RPC object per line.

```bash
cargo install --path crates/server
patchouli serve --endpoint <endpoint>
patchouli status --endpoint <endpoint>
patchouli stop --endpoint <endpoint>
patchouli config check config/patchouli.example.json
```
