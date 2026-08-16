# dsh-patchouli

Patchouli 同时提供与 Harness 无关的知识存储后端，以及面向
[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的通用记忆前端。具体记忆插件通过 DSH 进程内的 `update` / `retrieve` / `subscribe` 接口接入；官方 Agent Loop 适配器决定何时调用前两个接口，Session 与 Workspace Indexer 各自拥有独立插件边界，响应式 Consumer 可以持久化自己的订阅进度，本地存储型插件还可以按需连接 Patchouli daemon。

> 当前状态：Rust 后端已经实现类型化知识、事务、冲突解决、通用检索、cursor 变更订阅、SQLite/远程 provider 路由、managed Artifact 文件库和 daemon 生命周期；DSH 侧已经实现通用 Memory Service、Agent Loop 的逐点可配置 Hooks 与主动 Tool、图片/工作区文件摄取、高层响应式订阅和 Web cursor 持久化，以及 TypeScript storage client。Session 与 Workspace Indexer 已建立独立包，扫描业务逻辑尚未实现；已有一个仅用于闭环验证的 CRUD 直通 MemoryPlugin，但尚未提供通用的知识抽取 MemoryPlugin。

## 架构边界

```text
Official Agent Loop
  → @memorax-agent/dsh-patchouli-agent-loop
  → ctx.patchouliMemory
  → registered MemoryPlugin
       ├─ MemoraX / another remote API
       ├─ artifact-ingestor → attachments / fs
       └─ optional ctx.patchouli
            ← dsh-patchouli/storage
            → JSON-RPC daemon
            → BackendEngine
            → SQLite / remote provider

Reactive Consumer
  → ctx.patchouliMemory.subscribe
  → ctx.patchouliMemoryCursors
  → DSH storageDomain

DSH sessionQuery                 DSH workspaceRegistry + fs
  → session-indexer                → workspace-indexer
  → ctx.patchouliMemory            → ctx.patchouliMemory
```

仓库围绕九个边界组织：

1. **Common Memory Service**：根入口 `dsh-patchouli` 通过 `ctx.patchouliMemory` 暴露 `update` / `retrieve` / `subscribe`，并按插件注册时提供的过滤器完成路由和来源聚合；未提供过滤器的插件默认接收所有调用。
2. **Memory Plugin**：实现具体记忆语义，例如 MemoraX 或本地知识插件。高层 `update` 表示提交信息供插件吸收，不等同于实体 CRUD update。
3. **Agent Loop Adapter**：`@memorax-agent/dsh-patchouli-agent-loop` 通过 Hooks 和 Tools 定义调用时机，并构造包含可信来源身份、scope 和 JSON 属性的标准 `meta`。
4. **Artifact Ingestor**：`@memorax-agent/dsh-patchouli-artifact-ingestor` 将 DSH 图片附件引用和显式工作区文件请求解析成 managed Artifact；真实字节只通过 `attachments` / `fs` 与 `ctx.patchouli` 传输。
5. **Session Indexer**：`@memorax-agent/dsh-patchouli-session-indexer` 预留从 DSH `sessionQuery` 向 Memory Service 提交会话知识的独立边界；当前不执行扫描。
6. **Workspace Indexer**：`@memorax-agent/dsh-patchouli-workspace-indexer` 预留通过 DSH `workspaceRegistry` 和 `fs` 索引工作区的独立边界；当前不执行爬取或监听。
7. **Cursor Store**：`dsh-patchouli/cursor-store` 将 consumer、subscription、scope 和 plugin 四元组绑定到 DSH `storageDomain`。
8. **Optional Storage Client**：`dsh-patchouli/storage` 连接 daemon，通过 `ctx.patchouli` 暴露低层控制、CRUD、Artifact 上下行和 change stream；它不是 MemoryPlugin，也不会被默认加载。
9. **Storage Backend**：Harness-neutral 的 Rust daemon、协议、事务引擎和 provider 层。

详细职责见 [架构文档](docs/architecture.md)，知识字段与关系模型见 [事实模型](docs/knowledge-model.md)，后端配置见 [配置文档](docs/backend-configuration.md)。

Common Memory Service 仅供同一 DSH 进程中的插件调用，不暴露外部 bridge 或跨进程服务。其他 Harness 或外部应用需要各自实现适配器。DSH 内调用方不指定内部插件；每次调用的 `meta.source` 标识调用方类型和实例，`meta.scope` 标识语义作用域，`meta.requestId` 和 `meta.attributes` 承载可选的 JSON 安全上下文。第三方 MemoryPlugin 可在注册时提供基于 `operation + meta` 的同步过滤器；不匹配的插件从本次结果中省略，过滤器异常则作为该插件的路由失败返回或上报。

`update` 与 `retrieve` 的输入、成功返回值均为插件自有的 JSON `data`；核心 Service 保留“存 / 取 / 订阅”三个高层入口，但不解释每个插件的数据结构。私有包 `@memorax-agent/dsh-patchouli-crud-test-plugin` 使用这条边界，将测试请求原样转发给 `ctx.patchouli` 的 SQLite CRUD，并将 daemon 响应原样返回。该包不进入默认 bundle，也不是生产记忆实现。

## Agent Loop Consumer

默认 bundle 加载根 Memory Service、官方 Agent Loop 适配器、Artifact Ingestor 以及两个 Indexer 包。Artifact Ingestor 在可选的 `ctx.patchouli`、DSH `attachments` 与 `fs` 同时可用后启动；未启用本地存储时保持 pending。两个 Indexer 当前只固定包边界和服务依赖，不执行索引。

```yaml
- id: patchouli-agent-loop
  config:
    retrieve:
      sessionStart: false
      preStep: true
      turnStopping: false
      toolPostExecute: false
    store:
      agentCreated: false
      agentDisposed: false
      requestError: false
      agentError: false
      turnEnd: true
      toolResult: false
    modelTools:
      retrieve: true
      update: true
```

- 默认在每个 `agent/pre-step` 取数据，并在每个已提交的 `turn/end` 存数据；其他 Agent 与 Tool 点位显式开启。
- 可取点位为 `agent/session-start`、`agent/pre-step`、`agent/turn-stopping` 和 `tools/post-execute`；可存点位为 `agent/created`、`agent/disposed`、`agent/request-error`、`agent/error`、`session/turn-end` 和 `tools/result`。
- Adapter 将点位能够观察到的 Session、Agent、Hook、Tool 原始 JSON 数据交给 MemoryPlugin，不生成查询提示词，不抽取、总结或决定如何记忆。
- `memory_update` / `memory_retrieve` Tool 可分别关闭；默认都提供模型主动调用路径。
- `memory_update` 可用 `resources: [{ kind: "workspace-file", path, mediaType?, name?, role? }]` 表达文件摄取；路径必须解析在当前 Session 工作区内。默认的 `turn/end` 数据也会保留 DSH 图片附件引用，供 Artifact Ingestor 读取真实图片字节。
- `agent/session-start` 是官方非等待通知，因此召回是 best-effort 异步注入；其余可取点位遵循官方 Hook 的等待语义。后台存储按 Session 串行，`session/flush` 会等待已接纳的存储任务。

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
  {
    meta: {
      source: { type: 'consumer', id: 'knowledge-index' },
      scope,
    },
  },
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

storage client 当前提供 `status`、`checkpoint`、实体 `create/read/retrieve/update/delete`、managed Artifact 上下行，以及带 handler 的 `subscribe`。JSON-RPC 失败会保留为 `PatchouliRpcError` 的 `method`、`code`、`data` 和可选 `reason`。订阅 handle 的 `closed` 会报告 `unsubscribed`、`connection-lost` 或 `client-closed`；`unsubscribe()` 幂等。低层 handler 按 wire 顺序触发，但调用方仍负责 cursor 幂等、所需的异步串行和生命周期清理。

启用 storage 后，默认 bundle 中 pending 的 Artifact Ingestor 会自动注册。它使用 `ctx.attachments.readImage()` 摄取会话图片，使用 `ctx.fs.resolve/stat/readBytes()` 摄取显式工作区文件，并通过 `ctx.patchouli.uploadArtifact()` 写入 managed 文件库。图片或文件的默认单项上限为 32 MiB；后端 metadata 字段名可通过该插件的 `metaFields` 配置映射。

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

Web 配置中应出现 `patchouli`、`patchouli-agent-loop`、`patchouli-artifact-ingestor`、`patchouli-session-indexer`、`patchouli-workspace-indexer` 和 `patchouli-memory-cursors`。完整 daemon、三平台和 CI 操作见 [开发文档](docs/development.md)。

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
├── packages/
│   ├── agent-loop/           # 官方 Agent Loop 适配器
│   ├── artifact-ingestor/    # DSH 图片与工作区文件摄取
│   ├── crud-test-plugin/     # 测试用第三方 CRUD 直通插件
│   ├── protocol/             # Harness-neutral JSON-RPC 类型与 schema
│   ├── session-indexer/      # Session Indexer 插件边界
│   └── workspace-indexer/    # Workspace Indexer 插件边界
├── src/                      # common、storage 和 cursor DSH 前端
├── test/                     # 前端最小契约测试
├── integration/              # daemon + SQLite 真实闭环测试
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
