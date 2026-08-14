# dsh-patchouli

Patchouli 是面向 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的本地知识依赖。目标是在不要求模型发起 Tool Call 的前提下，按当前任务检索相关知识，并通过 Harness 原生、可记录的上下文链路注入模型请求。

> 当前状态：已提供通用 Memory Service，以及面向官方 Agent Loop 的 Consumer MVP。Consumer 会对真实用户输入执行自动 `retrieve`，并注册主动 `memory_update` / `memory_retrieve` Tool；尚未接入具体记忆实现或数据库后端。

## 设计方向

Patchouli 将以 monorepo 形式围绕四个边界逐步实现：

1. **Memory Service**：通过 `ctx.patchouliMemory` 暴露稳定的 `update` / `retrieve` 契约，并负责插件注册、路由和结果聚合。
2. **Memory Plugin**：实现具体的 `update` / `retrieve` 语义，例如 MemoraX 或本地记忆实现。
3. **Agent Loop Consumer**：通过 Tool、Hook 或 `agent/pre-step` 决定何时以及如何调用 Memory Service。
4. **Storage Backend**：为需要本地持久化的 Memory Plugin 提供独立的 Rust CRUD/change 能力。

数据库后端使用 Rust 实现，独立于 DeepSeek Harness；TypeScript 仅承担插件和客户端协议类型。

当前 MVP 同时提供自动上下文注入和主动 Tool；后续再根据真实使用效果确定默认策略。详细约束见 [架构文档](docs/architecture.md)。

## 环境要求

- Node.js `^22.19.0` 或 `>=24.0.0`
- pnpm `11`
- Rust stable
- DeepSeek Harness `0.1.0-rc.6` 或兼容版本

## 本地开发

```bash
pnpm install
pnpm check
cargo test --workspace
```

根插件和各 package 的构建产物位于各自的 `lib/`。生成可安装的 npm tarball：

```bash
pnpm pack
```

在本地 DeepSeek Harness profile 中安装当前 checkout：

```bash
dsh plugin --profile web add .
dsh --profile web --dump-config
```

配置中应出现 `patchouli` 和 `patchouli-agent-loop` 两个插件行。未注册具体 Memory Plugin 时，自动召回不会注入内容，主动 Tool 会返回没有可用插件。

## 仓库结构

```text
.
├── .github/workflows/ci.yml  # 验证与交付物打包
├── docs/                     # 架构和开发文档
├── crates/
│   └── backend/              # Rust 数据库后端核心
├── packages/
│   └── protocol/             # 与 Harness 无关的数据库 JSON-RPC 契约
├── src/                      # Cordis 插件源码
├── test/                     # 最小契约测试
├── cordis.patch.yml          # DeepSeek Harness bundle 配置层
├── package.json
└── tsconfig.json
```

## CI 与交付

- Pull Request 和普通分支提交在 GitHub-hosted runner 上执行安装、类型检查、构建和测试。
- `main` 分支提交或手动运行工作流时，在仓库已注册的 `self-hosted` runner 上构建 npm tarball，并上传为 Actions artifact。
- 自动安装到服务器上的某个 DSH profile 尚未启用；需要先确定目标 profile、持久部署目录和回滚方式。

开发约定见 [docs/development.md](docs/development.md) 和 [CONTRIBUTING.md](CONTRIBUTING.md)。

## License

[MIT](LICENSE)
