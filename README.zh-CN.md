<div align="center">
  <img width="100%" alt="Patchouli" src="assets/patchouli-banner-zh.png">

  <h1>Patchouli</h1>
  <p>
    <strong>面向 AI Harness 的知识与记忆中台。</strong>
    <br />
    通过统一服务连接应用、记忆插件与持久化知识存储。
  </p>

  [![CI](https://github.com/memorax-agent/dsh-patchouli/actions/workflows/ci.yml/badge.svg)](https://github.com/memorax-agent/dsh-patchouli/actions/workflows/ci.yml)
  [![License: MIT](https://img.shields.io/badge/License-MIT-2f6f4e.svg)](LICENSE)
  [![Node.js](https://img.shields.io/badge/Node.js-%5E22.19.0%20%7C%7C%20%3E%3D24-2f6f4e?logo=nodedotjs&logoColor=white)](package.json)
  [![Rust](https://img.shields.io/badge/Rust-stable-b55b3d?logo=rust&logoColor=white)](Cargo.toml)

  [English](README.md) / **简体中文**
</div>

## 概述

Patchouli 是一个知识与记忆中台。应用通过稳定的 `update`、`retrieve`、`subscribe` 服务发起调用，注册插件提供具体记忆语义，独立的 Rust 后端负责事务化存储。

[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 是目前首个受支持的集成，数据库后端本身不依赖任何 Harness。

## 核心能力

- 提供带插件路由、聚合和来源标记的 DSH Memory Service。
- 通过独立连接器插件接入 Agent Loop Hook 与模型 Tool。
- 支持本地或远程记忆/知识插件。
- 将图片和工作区文件摄取为类型化 Artifact。
- 提供持久化 cursor 与响应式订阅。
- 提供支持 SQLite、远程 Provider、冲突处理、变更流和故障恢复的事务化 Rust 后端。

## 详细文档

安装、DSH 集成、架构、一致性配置、知识模型和后端运维说明都在
[Patchouli 文档站](https://memorax-agent.github.io/dsh-patchouli/)中。

文档源码位于 [`docs/`](docs/)，规范性的 JSON-RPC 契约位于
[`packages/protocol/SPEC.md`](packages/protocol/SPEC.md)。

## 当前状态

Memory Service、Agent Loop 连接器、Artifact Ingestor、事务化 Rust 后端、
SQLite 与远程 Provider、冲突处理、变更流和生命周期恢复均已实现。Session
与 Workspace Indexer 当前只完成包边界，尚未提供通用 Knowledge 抽取插件。

## 许可

项目采用 [MIT License](LICENSE)。

## 这个插件的名字是什么意思？？？

名称直接来自 [Patchouli Knowledge](https://en.touhouwiki.net/wiki/Patchouli_Knowledge)，同时致敬广为人知的 Minecraft 模组 [Patchouli](https://github.com/VazkiiMods/Patchouli)。
