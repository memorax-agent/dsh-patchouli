---
layout: home

hero:
  name: Patchouli
  text: Knowledge that outlives the harness.
  tagline: One coordination layer for DSH applications, memory plugins, and durable transactional storage.
  image:
    src: /patchouli-icon-color.png
    alt: A purple isometric bookshelf with a yellow crescent
  actions:
    - theme: brand
      text: Get started
      link: /getting-started
    - theme: alt
      text: Understand the architecture
      link: /architecture

features:
  - title: Coordination first
    details: Applications call one update, retrieve, and subscribe service while registered plugins own memory semantics.
  - title: Storage without lock-in
    details: The Rust backend is harness-neutral, transactional, and routes scopes to local SQLite or remote providers.
  - title: Reactive by design
    details: Durable cursors, retained change streams, and explicit lifecycle behavior make live consumers recoverable.
---

## One boundary for knowledge

Patchouli separates when knowledge is needed, what a memory implementation does,
and where durable state lives. DeepSeek Harness is the first integration, not a
constraint on the backend.

Start with the [installation guide](./getting-started.md), then follow the
[architecture](./architecture.md) to choose the pieces your deployment needs.
