# Architecture

## Status

Patchouli is currently an installable Cordis plugin skeleton. This document records the intended capability boundary; it does not claim that retrieval is implemented yet.

## Goal

Provide DeepSeek Harness with local, workspace-aware knowledge context without requiring the model to decide to call a knowledge tool.

```text
Knowledge Provider
       │
       ▼
Knowledge Service on ctx
       │
       ▼
Context Consumer ── agent/pre-step ── Session Log ── Model request
```

## Capability roles

### Service Definition

The Service Definition owns the stable query contract and shared result vocabulary. It contains no storage, indexing, Agent, or prompt logic.

Expected responsibilities:

- accept a query, workspace identity, result limit, and cancellation signal;
- return bounded hits with stable identifiers, source locations, content, and scores;
- expose no model-facing schema by itself.

### Service Provider

The database backend is implemented in Rust and owns persistence plus the generic entity CRUD/change stream. It must not depend on Agent lifecycle, prompt assembly, DeepSeek Harness, or Cordis. Provider and retrieval code consume this backend through the versioned JSON-RPC contract.

The frontend binding is stateless with respect to database policy. Backend calls follow this boundary:

```text
JSON-RPC adapter
    -> backend controller
    -> configured policy engine
    -> database provider primitives
```

The controller owns schemas, identity extraction, consistency selection, logical transaction/batch state, conflicts and publication. A database provider does not interpret business fields and is never called directly by the frontend adapter.

### Context Consumer

The Consumer listens to `agent/pre-step`, calls `next()`, retrieves against the admitted user messages, and appends one plugin-sourced user message to the returned decision.

The injected message must be:

- bounded by an explicit result and byte budget;
- tagged with plugin provenance and section metadata;
- appended rather than inserted into earlier history;
- admitted through the normal Session Log path so the model-visible request can be reconstructed;
- emitted once per user turn unless a later requirement justifies another retrieval.

## Default model interface

Patchouli will not expose knowledge retrieval as a model tool by default. Human commands, administration UI, or optional tools may be added as separate Consumers only when there is a concrete use case.

## Initial delivery sequence

1. A single local Provider and automatic Context Consumer working end to end.
2. Durable ingestion and incremental indexing.
3. Operational surfaces such as status, rebuild, and inspection.

The repository is a monorepo. Rust backend crates live under `crates/`; JavaScript/TypeScript protocol and Harness packages live under `packages/`. The root package remains the DeepSeek Harness plugin until it is moved into its own package.
