# dsh-patchouli

Patchouli 同时提供与 Harness 无关的知识存储后端，以及面向
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的通用记忆前端。具体记忆插件通过统一的 `update` / `retrieve` 接口接入；官方 Agent Loop Consumer 决定何时调用这些接口，本地存储型插件还可以按需连接 Patchouli daemon。

> 当前状态：Rust 后端已经实现类型化知识、事务、冲突解决、通用检索、cursor 变更订阅、SQLite/远程 provider 路由和 daemon 生命周期；DSH 侧已经实现通用 Memory Service、主动 Tool、自动 retrieve、可选的 turn-end 自动 update，以及 TypeScript storage client 的 entity retrieve 和响应式订阅。尚未提供具体 MemoryPlugin。

## 架构边界

```text
Official Agent Loop
  → dsh-patchouli/agent-loop
  → ctx.patchouliMemory
  → registered MemoryPlugin
       ├─ MemoraX / another remote API
       └─ optional ctx.patchouli
            ← dsh-patchouli/storage
            → JSON-RPC daemon
            → BackendEngine
            → SQLite / remote provider
```

仓库围绕五个边界组织：

1. **Common Memory Service**：根入口 `dsh-patchouli` 通过 `ctx.patchouliMemory` 暴露 `update` / `retrieve`，并负责插件注册、路由和结果聚合。
2. **Memory Plugin**：实现具体记忆语义，例如 MemoraX 或本地知识插件。高层 `update` 表示提交信息供插件吸收，不等同于实体 CRUD update。
3. **Agent Loop Consumer**：`dsh-patchouli/agent-loop` 通过 Hooks 和 Tools 定义调用时机及标准 metadata。
4. **Optional Storage Client**：`dsh-patchouli/storage` 连接 daemon，通过 `ctx.patchouli` 暴露低层控制和 CRUD；它不是 MemoryPlugin，也不会被默认加载。
5. **Storage Backend**：Harness-neutral 的 Rust daemon、协议、事务引擎和 provider 层。

详细职责见 [架构文档](docs/architecture.md)，知识字段与关系模型见 [事实模型](docs/knowledge-model.md)，后端配置见 [配置文档](docs/backend-configuration.md)。

## Agent Loop Consumer

默认 bundle 加载根 Memory Service 和官方 Agent Loop Consumer。未注册具体 MemoryPlugin 时，自动召回不会注入内容，主动 Tool 会返回没有可用插件。

```yaml
- id: patchouli-agent-loop
  config:
    autoRetrieve: true
    autoUpdate: false
```

- `autoRetrieve` 默认开启，在 `agent/pre-step` 对直接用户输入执行一次 retrieve。
- `autoUpdate` 默认关闭；开启后，在 `completed` 或 `max-tokens` 的 `turn/end` 已经提交到 Session Log 后执行 update。
- 自动 update 只采集直接用户消息和 assistant 可见文本，不回灌 recall、reasoning、tool call 或 tool result。
- `memory_update` / `memory_retrieve` Tool 始终提供主动调用路径。

## 可选本地存储

安装 daemon/CLI，并准备默认 policy 与 provider 配置：

```bash
cargo install --path crates/server
mkdir -p "$HOME/.patchouli"
cp config/patchouli.default.json "$HOME/.patchouli/config.json"
cp config/providers.local.json "$HOME/.patchouli/providers.json"
```

`dsh-patchouli/storage` 不在默认 bundle 中。需要本地存储的 MemoryPlugin 可以显式启用它：

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

storage client 当前提供 `status`、`checkpoint`、实体 `create/read/retrieve/update/delete`，以及带 handler 的 `subscribe` / `unsubscribe`。handler 按 wire 顺序触发；调用方负责 cursor 幂等与所需的异步串行，并应显式取消订阅。断连或 storage plugin 卸载时，连接上的 handler 会统一清理。

## 本地开发

环境要求：Node.js `^22.19.0` 或 `>=24.0.0`、pnpm 11、Rust stable，以及使用 Agent Loop Consumer 时兼容 DeepSeek Harness `0.1.0-rc.6` 的运行时。

```bash
pnpm install
pnpm check
cargo test --workspace
```

生成 npm 安装包：

```bash
pnpm pack
```

在本地 DSH profile 中安装当前 checkout：

```bash
dsh plugin --profile web add .
dsh --profile web --dump-config
```

配置中应出现 `patchouli` 和 `patchouli-agent-loop`。完整 daemon、三平台和 CI 操作见 [开发文档](docs/development.md)。

## 仓库结构

```text
.
├── crates/
│   ├── backend/              # 事务、冲突、一致性和知识模型
│   ├── provider/             # provider 公共边界
│   ├── provider-remote/      # 远程 provider 认证传输
│   ├── provider-router/      # scope 路由
│   ├── provider-sqlite/      # SQLite adapter
│   └── server/               # daemon、IPC 和 CLI
├── packages/protocol/        # Harness-neutral JSON-RPC 类型与 schema
├── src/                      # common、storage 和 agent-loop 前端
├── test/                     # 前端最小契约测试
├── config/                   # backend policy 与 provider 配置
├── docs/                     # 架构、模型和开发文档
└── cordis.patch.yml          # 默认 DSH bundle
```

## CI 与交付

- PR 和普通分支在 GitHub-hosted Linux、macOS、Windows runner 上检查 Node 与 Rust。
- trusted `main` 和手动 workflow 会打包 DSH plugin 与 protocol artifact。
- `v*` tag 构建并发布三平台 daemon。
- trusted `main` 或手动 workflow 可在 self-hosted runner 部署 daemon；CI 不修改 DSH profile。

开发约定见 [docs/development.md](docs/development.md) 和 [CONTRIBUTING.md](CONTRIBUTING.md)。

## License

[MIT](LICENSE)
