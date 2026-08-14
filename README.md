# Patchouli

Patchouli 是与具体 Harness 无关的本地知识数据库后端。它通过版本化
JSON-RPC/IPC 提供类型化知识、事务、冲突解决、检索和响应式变更；
DeepSeek Harness/Cordis 插件和 Agent Loop 接入由前端协作者维护。

> 当前状态：已经具备 Rust `BackendEngine`、本地 IPC、控制 CLI、SQLite provider 生命周期、可配置的一致性/冲突计划，以及 `Knowledge`/`KnowledgeRelation` v1 类型和数据库条目。默认 CRUD 使用真实 SQLite 事务：Knowledge 内容通过 Automerge 合并，其他字段保留 MVCC 多版本。后端支持持久化跨 RPC work unit、固定基线、暂存读取、批次原子发布、持久化幂等、cursor 变更订阅、通用实体检索，以及按 scope 将请求稳定路由到 `local` 或经过认证的远程 provider。DeepSeek Harness 插件的业务接口和 Agent Loop 接线由前端协作者实现。

daemon 会独占数据库实例，记录运行代次和正常/异常关闭状态。正常停止会先停止接收连接、关闭现有 IPC 会话，再执行 SQLite WAL checkpoint、持久化干净关闭标记并释放数据库锁；异常退出后的下一次启动由 SQLite WAL 自动恢复已提交事务，并在状态接口中报告恢复事实。

## 设计方向

Patchouli 以 monorepo 形式围绕三个后端边界实现：

1. **Protocol**：定义 Harness-neutral 的 CRUD、检索、订阅和控制契约。
2. **Backend Engine**：执行配置化 scope、一致性、幂等、事务和冲突策略。
3. **Provider**：负责持久化、检索、change-log 和运行生命周期，不依赖 Agent 或 Prompt。

数据库后端使用 Rust 实现；TypeScript package 只提供通用协议类型。

默认模型界面是自动上下文注入，而不是知识库工具。详细约束见 [架构文档](docs/architecture.md)。
知识事实字段、关系方向和 SQLite 条目见 [事实模型](docs/knowledge-model.md)。

## 环境要求

- Node.js `^22.19.0` 或 `>=24.0.0`
- pnpm `11`
- Rust stable

## 本地开发

```bash
pnpm install
pnpm check
cargo test --workspace
```

`packages/protocol` 的构建产物位于其 `lib/`。根目录仍保留由前端协作者
接管的 Cordis 连接骨架，但它不属于后端交付范围。

默认数据库 scope 是 `workspace_id + user_id`，所以知识可以跨 channel 复用；`channel_id` 仅作为会话控制 metadata。

先安装 daemon/CLI：

```bash
cargo install --path crates/server
```

将 backend policy 放到插件的默认配置位置：

```bash
mkdir -p "$HOME/.patchouli"
cp config/patchouli.default.json "$HOME/.patchouli/config.json"
cp config/providers.local.json "$HOME/.patchouli/providers.json"
```

daemon 通过 `patchouli serve` 同时加载业务 policy 和 provider/routing 配置。
`providers.json` 中唯一的本地 provider 固定命名为 `local`，相对数据库路径以该文件目录为基准。
它由 `patchouli stop --endpoint <endpoint>`
显式停止。运行期间可用
`patchouli checkpoint --endpoint <endpoint>` 主动执行一次 WAL checkpoint。详细的三平台命令见
[开发文档](docs/development.md)。

## 仓库结构

```text
.
├── .github/workflows/ci.yml  # 验证、三平台发布和 daemon 部署
├── docs/                     # 架构和开发文档
├── crates/
│   ├── backend/              # Rust 数据库后端核心契约
│   ├── provider/             # 数据库 provider 公共边界
│   ├── provider-remote/      # 远程 provider 认证传输
│   ├── provider-router/      # 基于 scope 的稳定路由
│   ├── provider-sqlite/      # 默认 SQLite adapter
│   └── server/               # 跨平台 daemon、IPC 和控制 CLI
├── packages/
│   └── protocol/             # 与 Harness 无关的数据库 JSON-RPC 契约
├── src/                      # 前端协作者维护的 Cordis 连接骨架
├── test/                     # 连接骨架契约测试
├── package.json
└── tsconfig.json
```

## CI 与交付

- Pull Request 和普通分支提交在 GitHub-hosted runner 上执行安装、类型检查、构建和测试。
- `main` 分支提交或手动运行工作流时，在已注册的 `self-hosted` runner 上部署 Rust daemon，并执行状态健康检查；失败时恢复上一二进制。
- 同一工作流在 Linux、macOS 和 Windows runner 上构建 daemon；`v*` tag 会把三平台二进制发布到 GitHub Release。
- CI 不安装或修改 DSH profile。

开发约定见 [docs/development.md](docs/development.md) 和 [CONTRIBUTING.md](CONTRIBUTING.md)。

## License

[MIT](LICENSE)
