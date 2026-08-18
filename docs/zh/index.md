---
layout: home

hero:
  name: Patchouli
  text: Knowledge that outlives the harness.
  tagline: |-
    把 DeepSeek Harness 的记忆、知识组件接到同一个入口，
    并用独立数据库保存它们需要的数据。
  image:
    src: /patchouli-icon-color.png
    alt: 带有黄色新月的紫色等轴测书架
  actions:
    - theme: brand
      text: 快速开始
      link: /zh/getting-started
    - theme: alt
      text: 了解架构
      link: /zh/architecture

features:
  - title: 一次调用，多路记忆
    details: update、retrieve 与 subscribe 会送到匹配的插件；每份结果都保留来源，失败也不会影响其他插件。
  - title: 原子、一致、冲突可控
    details: 跨调用的知识操作可一次发布；并发写入可按配置用 Automerge 合并、用 MVCC 保留版本，或直接拒绝。
  - title: 知识不止文本
    details: 文件、图片、Embedding 与外部索引都可作为有类型的 Artifact；字节可由 Patchouli 托管，也可只保留原来的索引。
---

## 各做各自擅长的事

Patchouli 分开处理“何时调用知识”“插件如何处理数据”和“数据保存在哪里”。
DeepSeek Harness 是第一个接入方，数据库后端也可以服务于其他 Harness。

从[快速开始](./getting-started.md)入手，再阅读[架构](./architecture.md)，
选择适合当前部署的组件。
