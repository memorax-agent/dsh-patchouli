---
layout: home

hero:
  name: Patchouli
  text: Knowledge that outlives the harness.
  tagline: |-
    协调你的 DeepSeek Harness 记忆、知识组件，
    并提供统一、持久的数据库。
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
    details: 将 update、retrieve 与 subscribe 调用分发给匹配的插件，同时保留每路结果的插件归属与独立错误。
  - title: 原子、一致、冲突可控
    details: 让跨调用知识操作原子发布、按配置保证一致性，并通过 Automerge、MVCC 或严格拒绝，对多插件并发写进行合并、保留或阻止。
  - title: 知识不止文本
    details: 将文件、图片、Embedding 与外部索引统一表示为有类型、受 Scope 约束的 Artifact，可由 Patchouli 托管，也可保留原位索引。
---

## 为知识建立统一边界

Patchouli 将“何时需要知识”“记忆实现如何处理数据”和“持久状态保存在哪里”
拆分为独立职责。DeepSeek Harness 是首个集成对象，但不会限制数据库后端。

从[快速开始](./getting-started.md)入手，再阅读[架构](./architecture.md)，
选择适合当前部署的组件。
