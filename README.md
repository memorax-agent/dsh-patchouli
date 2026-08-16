<div align="center">
  <img width="100%" alt="Patchouli" src="assets/patchouli-banner-en.png">

  <h1>Patchouli</h1>
  <p>
    <strong>A local memory and knowledge hub for DeepSeek Harness.</strong>
    <br />
    Integrates heterogeneous Agent data augmentation while keeping data and algorithms decoupled.
  </p>

  **English** · [简体中文](README.zh-CN.md)

  [![Documentation](https://img.shields.io/badge/docs-read-75439a?logo=readthedocs&logoColor=white)](https://memorax-agent.github.io/dsh-patchouli/)
  [![CI](https://github.com/memorax-agent/dsh-patchouli/actions/workflows/ci.yml/badge.svg)](https://github.com/memorax-agent/dsh-patchouli/actions/workflows/ci.yml)
  [![License: MIT](https://img.shields.io/badge/license-MIT-2f6f4e.svg)](LICENSE)
  [![Node.js](https://img.shields.io/badge/Node.js-%5E22.19.0%20%7C%7C%20%3E%3D24-2f6f4e?logo=nodedotjs&logoColor=white)](https://memorax-agent.github.io/dsh-patchouli/installation)
  [![Rust](https://img.shields.io/badge/Rust-stable-b55b3d?logo=rust&logoColor=white)](https://memorax-agent.github.io/dsh-patchouli/installation)
</div>

## Overview

Patchouli exposes one `update` / `retrieve` / `subscribe` service inside
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness). Connectors
provide trusted runtime data, memory and knowledge plugins own their algorithms,
and an independent Rust backend provides durable transactional storage.

DeepSeek Harness is the first supported integration. The database backend
remains harness-neutral.

## Features

- Common Memory Service with plugin-owned routing, aggregation, and provenance.
- Agent Loop connector with configurable hooks and model tools.
- Pluggable local or remote memory and knowledge implementations.
- Managed image and workspace-file ingestion as typed Artifacts.
- Durable subscriptions and a transactional Rust backend with SQLite and remote providers.

## What does the plugin's name mean???

The name refers directly to
[Patchouli Knowledge](https://en.touhouwiki.net/wiki/Patchouli_Knowledge), and
also pays tribute to the widely known Minecraft mod
[Patchouli](https://github.com/VazkiiMods/Patchouli).
