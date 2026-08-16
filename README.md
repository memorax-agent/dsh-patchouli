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

## Install and use

Requires Node.js `^22.19.0 || >=24`, pnpm 11, and a DeepSeek Harness runtime
compatible with `0.1.0-rc.6`. Until the first packaged release, install the
current source branch:

```bash
git clone --branch main --single-branch https://github.com/memorax-agent/dsh-patchouli.git
cd dsh-patchouli
corepack enable
pnpm install
dsh plugin --profile web add .
dsh --profile web --dump-config
```

The last command should list `patchouli` and its connector plugins. Patchouli is
middleware: register at least one compatible memory or knowledge plugin to
handle the routed `update`, `retrieve`, and `subscribe` calls. By default, the
Agent Loop connector retrieves before each agent step, stores completed turns,
and exposes memory update and retrieval tools to the model.

The transactional database backend is optional. With Rust stable and a C
toolchain installed, build it from the same checkout and initialize its local
home:

```bash
cargo install --locked --path crates/server
patchouli-db init --root "$HOME/.patchouli"
```

Then enable the storage client in the DSH profile; it connects to the local
daemon and starts it when needed:

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

See the [Getting started guide](https://memorax-agent.github.io/dsh-patchouli/getting-started)
for configuration and platform-specific details.

## What does the plugin's name mean???

The name refers directly to
[Patchouli Knowledge](https://en.touhouwiki.net/wiki/Patchouli_Knowledge), and
also pays tribute to the widely known Minecraft mod
[Patchouli](https://github.com/VazkiiMods/Patchouli).
