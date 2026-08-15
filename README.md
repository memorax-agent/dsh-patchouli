# dsh-patchouli

Patchouli 同时提供与 Harness 无关的知识存储后端，以及面向
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的通用记忆前端。具体记忆插件通过统一的 `update` / `retrieve` / `subscribe` 接口接入；官方 Agent Loop Consumer 决定何时调用前两个接口，响应式 Consumer 可以持久化自己的订阅进度，本地存储型插件还可以按需连接 Patchouli daemon。

> 当前状态：Rust 后端已经实现类型化知识、事务、冲突解决、通用检索、cursor 变更订阅、SQLite/远程 provider 路由和 daemon 生命周期；DSH 侧已经实现通用 Memory Service、主动 Tool、自动 retrieve、可选的 turn-end 自动 update、高层响应式订阅和 Web cursor 持久化，以及 TypeScript storage client 的 entity retrieve 和低层响应式订阅。尚未提供具体 MemoryPlugin。

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

Reactive Consumer
  → ctx.patchouliMemory.subscribe
  → ctx.patchouliMemoryCursors
  → DSH storageDomain
```

仓库围绕六个边界组织：

1. **Common Memory Service**：根入口 `dsh-patchouli` 通过 `ctx.patchouliMemory` 暴露 `update` / `retrieve` / `subscribe`，并负责插件注册、路由和来源标记。
2. **Memory Plugin**：实现具体记忆语义，例如 MemoraX 或本地知识插件。高层 `update` 表示提交信息供插件吸收，不等同于实体 CRUD update。
3. **Agent Loop Consumer**：`dsh-patchouli/agent-loop` 通过 Hooks 和 Tools 定义调用时机及标准 metadata。
4. **Cursor Store**：`dsh-patchouli/cursor-store` 将 consumer、subscription、scope 和 plugin 四元组绑定到 DSH `storageDomain`。
5. **Optional Storage Client**：`dsh-patchouli/storage` 连接 daemon，通过 `ctx.patchouli` 暴露低层控制、CRUD 和 change stream；它不是 MemoryPlugin，也不会被默认加载。
6. **Storage Backend**：Harness-neutral 的 Rust daemon、协议、事务引擎和 provider 层。

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

## 响应式订阅

响应式 Consumer 先绑定自己的 cursor 空间，再订阅当前已注册且实现了 `subscribe` 的 MemoryPlugin：

```ts
const scope = '/workspace/example'
const cursorStore = ctx.patchouliMemoryCursors.bind({
  consumerId: 'knowledge-index',
  subscriptionKey: 'live-memory',
  scope,
})

const subscription = await ctx.patchouliMemory.subscribe(
  { scope, metadata: { source: 'knowledge-index' } },
  async ({ pluginId, cursor, memoryId, metadata }) => {
    await applyChangeIdempotently({ pluginId, cursor, memoryId, metadata })
  },
  {
    cursorStore,
    signal,
    onError({ pluginId, error }) {
      reportSubscriptionFailure(pluginId, error)
    },
  },
)
```

每个插件的初始 boundary 会先保存；同一插件的事件串行执行，相同 cursor 会跳过，handler 成功后才推进 durable cursor。插件之间互不阻塞。只有显式抛出的 `MemorySubscriptionError` 且 `retryable: true` 才会以 250 ms 起步、上限 30 秒加 jitter 的退避策略重连，并携带最后保存的 cursor；未知错误会停止对应插件的 worker。

`resetRequired` 同样只停止并通过 `onError` 上报，不会自动删除 cursor 或跳到最新位置。Consumer 应先完成 snapshot/resync，再停止旧订阅、调用 `cursorStore.delete(pluginId)` 并建立新订阅。handler 仍需幂等，以覆盖“副作用完成但 cursor 尚未成功保存”时的重放。`subscription.unsubscribe()` 是幂等的，会取消重试、取消底层订阅并等待已接收的 handler；订阅也会随调用方的 Cordis fiber 或传入的 `AbortSignal` 清理。

默认 bundle 会加载 `patchouli-memory-cursors`。Web profile 已提供 `storageDomain`，因此 cursor 写入其持久化存储；不含 storage stack 的 Headless profile 会让该插件保持 pending，但 `ctx.patchouliMemory`、Agent Loop Tools/Hooks 和非响应式的 update/retrieve 不受影响。Headless 响应式 Consumer 可以显式加载 DSH storage stack，或向 `subscribe` 传入自己的 `MemoryCursorStore`。

## 可选本地存储

推荐从 GitHub Release 安装预编译 daemon/CLI。macOS 和 Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.sh | sh
```

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.ps1 | iex
```

安装脚本会校验 Release SHA-256，并运行 `patchouli-db init` 创建默认 policy、provider 配置和运行目录，但不会覆盖已有配置。源码安装与完整的平台、升级和卸载说明见[安装文档](docs/installation.md)。

`dsh-patchouli/storage` 不在默认 bundle 中。需要本地存储的 MemoryPlugin 可以显式启用它：

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

storage client 当前提供 `status`、`checkpoint`、实体 `create/read/retrieve/update/delete`，以及带 handler 的 `subscribe`。JSON-RPC 失败会保留为 `PatchouliRpcError` 的 `method`、`code`、`data` 和可选 `reason`。订阅 handle 的 `closed` 会报告 `unsubscribed`、`connection-lost` 或 `client-closed`；`unsubscribe()` 幂等。低层 handler 按 wire 顺序触发，但调用方仍负责 cursor 幂等、所需的异步串行和生命周期清理。

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

Web 配置中应出现 `patchouli`、`patchouli-agent-loop` 和 `patchouli-memory-cursors`。完整 daemon、三平台和 CI 操作见 [开发文档](docs/development.md)。

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
