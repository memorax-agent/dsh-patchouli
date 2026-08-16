---
layout: home

hero:
  name: Patchouli
  text: Knowledge that outlives the harness.
  tagline: Coordinate your DeepSeek Harness memory and knowledge components with a unified, durable database
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
  - title: One call, many memories
    details: Route update, retrieve, and subscribe calls to matching plugins while preserving per-plugin provenance and isolated failures.
  - title: Atomic, consistent, conflict-aware
    details: Publish multi-call knowledge operations atomically, enforce configured consistency, and merge, preserve, or reject concurrent plugin writes through Automerge, MVCC, or strict rejection.
  - title: Knowledge beyond text
    details: Represent files, images, embeddings, and external indexes as typed, scoped Artifacts—managed by Patchouli or referenced in place.
---

## One boundary for knowledge

Patchouli separates when knowledge is needed, what a memory implementation does,
and where durable state lives. DeepSeek Harness is the first integration, not a
constraint on the backend.

Start with the [installation guide](./getting-started.md), then follow the
[architecture](./architecture.md) to choose the pieces your deployment needs.
