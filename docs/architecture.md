# Architecture

## Status

Patchouli currently provides the common Cordis Memory Service and an MVP Consumer for the official Agent Loop. Concrete memory plugins and storage connectivity are not implemented yet.

## Goal

Provide DeepSeek Harness with a common memory capability whose concrete plugins implement update and retrieval without coupling them to Agent Loop policy.

```text
Agent Loop Consumer
       │ update / retrieve
       ▼
Memory Service on ctx ── route / aggregate ── Memory Plugins
                                                  │
                                  optional CRUD ──┴── external API
```

## Capability roles

### Common Memory Service

The common frontend is registered as `ctx.patchouliMemory`. It owns the stable `update` / `retrieve` contract, the Memory Plugin registry, routing, and per-plugin outcomes. It contains no storage, Agent, or prompt logic.

Current responsibilities:

- accept an opaque scope plus update messages or a retrieval query;
- call registered Memory Plugins without interpreting their implementation;
- preserve plugin provenance and failures in the aggregate result;
- expose no model-facing schema by itself.

### Memory Plugins

A concrete Memory Plugin implements both high-level operations. `update` means submitting information for the plugin to incorporate according to its own memory semantics; it is not entity replacement. `retrieve` returns provider-local hits whose scores are not compared across plugins by the common frontend.

A MemoraX plugin may call the MemoraX API directly. A local plugin may instead consume the storage backend through its versioned JSON-RPC contract. Storage CRUD types do not appear in the common Memory Service contract.

### Storage Backend

The database backend is implemented in Rust and owns persistence plus the generic entity CRUD/change stream. It must not depend on Agent lifecycle, prompt assembly, DeepSeek Harness, or Cordis. Local Memory Plugins consume this backend through the versioned JSON-RPC contract.

The frontend binding is stateless with respect to database policy. Backend calls follow this boundary:

```text
JSON-RPC adapter
    -> backend controller
    -> configured policy engine
    -> database provider primitives
```

The controller owns schemas, identity extraction, consistency selection, logical transaction/batch state, conflicts and publication. A database provider does not interpret business fields and is never called directly by the frontend adapter.

### Agent Loop Consumer

The Consumer is a separate `dsh-patchouli/agent-loop` Cordis plugin. It registers model-facing `memory_update` and `memory_retrieve` tools and listens to `agent/pre-step` for automatic retrieval. Tool scope is derived from the session working directory, falling back to the session id, rather than accepted from model input.

The retrieval Hook calls `next()`, extracts text only from directly sourced user messages, retrieves through the common service, and appends one plugin-sourced recall message to the returned decision. Tool continuations, rejected steps, empty results, and retrieval failures inject nothing.

The injected message must be:

- bounded by an explicit result and character budget;
- tagged with plugin provenance and the `recall` context form;
- appended rather than inserted into earlier history;
- admitted through the normal Session Log path so the model-visible request can be reconstructed;
- emitted once per admitted direct-user batch; tool continuations do not repeat retrieval.

## MVP model interface

The current Consumer enables automatic retrieval and also exposes explicit update/retrieve tools. Whether automatic retrieval remains enabled by default is intentionally deferred until the end-to-end MemoraX path can be evaluated.

## Initial delivery sequence

1. Common Memory Service and Memory Plugin registry.
2. Official Agent Loop Consumer with Hook and Tool paths.
3. A MemoraX plugin working end to end through the common service and Consumer.
4. Local storage-backed plugins and operational surfaces such as status, rebuild, and inspection.

The repository is a monorepo. Rust backend crates live under `crates/`; JavaScript/TypeScript protocol and Harness packages live under `packages/`. The root package remains the DeepSeek Harness plugin until it is moved into its own package.
