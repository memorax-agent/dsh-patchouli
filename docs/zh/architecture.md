<script setup lang="ts">
import PatchouliArchitecture from '../components/PatchouliArchitecture.vue'
import PatchouliConceptFlow from '../components/PatchouliConceptFlow.vue'
</script>

# 架构

## 当前状态

Patchouli 由两个已经实现的 Surface 组成：

- 与 Harness 无关的 Rust 后端：事务 CRUD、类型化知识、Automerge/MVCC 冲突
  处理、词法检索、可保留变更流、SQLite 与认证远程 Provider、确定性 Scope 路由
  和 Daemon 生命周期恢复；
- DSH 统一 Memory Service：响应式订阅、持久化 Consumer Cursor、官方 Agent Loop
  适配器、托管 Artifact 摄取，以及 Session/Workspace Indexer 的包边界。

Session 和 Workspace Indexer 的实际行为、通用 Knowledge 抽取/检索 MemoryPlugin
尚未实现。可选 TypeScript 存储客户端已经暴露后端协议中的控制、CRUD、实体检索、
Artifact 传输、结构化 RPC 错误和基于 Cursor 的订阅。

## 目标

为 DSH 提供统一记忆能力，同时让 Agent Loop 策略、记忆语义和存储机制可以独立替换。

<ClientOnly>
  <PatchouliArchitecture locale="zh" />
  <template #fallback>正在加载架构图……</template>
</ClientOnly>

`ctx.patchouliMemory` 是 DSH 进程内服务，不通过外部 Bridge 暴露。其他 Harness 和
外部应用需要自己的适配器。

## 能力职责

### 统一 Memory Service

根插件 `dsh-patchouli` 注册 `ctx.patchouliMemory`，负责稳定的
`update` / `retrieve` / `subscribe` 契约、MemoryPlugin 注册、路由和来源标记。
它不包含存储、Agent 或 Prompt 逻辑。

服务接收含可信 Source Identity、Opaque Scope、可选 JSON Attributes 和操作参数的
统一 Envelope。每个插件可注册同步纯 Filter；无 Filter 保持广播。Filter 异常只会
成为该插件的路由失败。聚合结果保留插件来源并隔离错误，服务本身不解释插件的
`data`，也不直接暴露模型 Schema。

### MemoryPlugin

MemoryPlugin 实现高层 `update` 与 `retrieve`，并可选实现 `subscribe`。`update` 表示
提交信息供插件按自己的语义吸收，而不是替换数据库实体。调用方不能选择插件 ID；
理解自身能力的插件通过 Filter 控制路由。

远程插件可以直接调用外部 API，本地插件可以使用可选存储客户端。存储 CRUD 类型
不会进入统一 Memory Service 契约。

Artifact Ingestor 是受限的本地例外：它只处理 Update，把 DSH 资源引用转换为托管
Artifact。Session 图片经 `ctx.attachments` 解析，`workspace-file` 经 `ctx.fs`
解析并验证工作区边界，然后通过 `ctx.patchouli.uploadArtifact` 传输。依赖缺失时
Cordis Fiber 保持 Pending。

订阅以每个插件独立串行处理事件，并在 Handler 成功后保存 Cursor。可重试错误使用
250 ms 到 30 秒的带抖动指数退避，从最后持久化 Cursor 重连；Fatal、未知错误和
`resetRequired` 只停止对应插件。Reset 必须由 Consumer 显式完成快照、替换订阅并
删除该插件 Cursor。`unsubscribe` 会取消重试和 Provider Handle，并排空已接收任务。

### Agent Loop Consumer

`@memorax-agent/dsh-patchouli-agent-loop` 将官方 Agent、Session、Tool 扩展点映射到
统一服务，并注册 `memory_update` 与 `memory_retrieve`。检索点包括
`agent/session-start`、`agent/pre-step`、`agent/turn-stopping`、
`tools/post-execute`；存储点包括 Agent 生命周期与错误、已提交 `session/turn-end`
和 `tools/result`。默认只启用 `agent/pre-step`、`session/turn-end` 和两个模型工具。

适配器发送点位上可见的无损 JSON，不构造检索 Prompt、不总结、不决定哪些信息应
成为记忆。成功检索会为每个插件产生一条带来源的 JSON Recall Message，并通过官方
上下文通道注入。`session-start` 是 Fire-and-forget；其他等待型 Hook 保持官方语义。

每次调用都包含规范化 `meta`，Source 固定为
`{ type: "agent-loop", id: "dsh-patchouli-agent-loop" }`。Scope 来自 Session
工作目录，缺失时回退到 Session ID；Attributes 记录 Point、Session 和位置。模型
不能伪造这个可信 Envelope。

### Indexer 包

Session Indexer 依赖 `ctx.patchouliMemory + ctx.sessionQuery`，未来负责 Session 扫描
和增量提交。Workspace Indexer 依赖
`ctx.patchouliMemory + ctx.workspaceRegistry + ctx.fs`，未来负责工作区遍历和变更
观察。目前两者只定义包边界；依赖缺失时保持 Pending，不使用替代数据源。

### 持久化 Consumer Cursor

`dsh-patchouli/cursor-store` 注册 `ctx.patchouliMemoryCursors`。`bind` 绑定
`consumerId + subscriptionKey + scope`，再加入 Plugin ID 形成四段身份。服务只保存
Opaque Cursor，不负责 Consumer Snapshot 或 Reset 策略。

Web Assembly 提供 `storageDomain`，因此 Cursor 持久化到 DSH 存储；Headless 缺少
存储栈时仅 Cursor Fiber Pending，Memory Service、Agent Loop 与 Update/Retrieve
仍可使用。Headless Consumer 也可直接提供其他 `MemoryCursorStore`。

### 可选存储客户端

`dsh-patchouli/storage` 注册 `ctx.patchouli`，连接既有 Daemon，并可在 Endpoint
不可用时启动一个。它不属于默认 Bundle，因此纯远程 MemoryPlugin 不依赖本地后端。

客户端提供状态、Checkpoint、通用实体 CRUD/检索、托管 Artifact 上下行和变更订阅。
`PatchouliRpcError` 保留 Method、Code、Data 与可选 Reason。低层客户端只按 Wire
顺序分发，不序列化异步 Handler、不持久化 Cursor、也不重连；这些职责由存储型
MemoryPlugin 映射到高层订阅契约。卸载插件只关闭 IPC，不停止独立 Daemon。

### 存储后端

Rust 后端拥有持久化、通用实体 CRUD、检索和变更流，不依赖 Agent 生命周期、Prompt、
DSH 或 Cordis。首个 Fact Vocabulary 包含 `artifact`、`knowledge` 和
`knowledge_relation`，详见[知识模型](knowledge-model.md)。

<ClientOnly>
  <PatchouliConceptFlow kind="backend" locale="zh" />
  <template #fallback>正在加载后端请求路径……</template>
</ClientOnly>

Controller 负责 Schema、身份提取、一致性规划、逻辑 Work Unit、冲突、幂等和发布。
`BackendEngine` 持有不可变已验证策略与一个 Provider Boundary，本身不保存跨请求
Transaction Map。Provider 负责持久生命周期：启动时取得独占所有权并恢复 SQLite，
关闭时排空连接、标记 Clean、Checkpoint WAL 并释放锁。Mutation 必须持久化后才能
返回成功。

Daemon 的 Artifact Store 是独立本地内容寻址文件库。Upload Commit 验证并发布字节，
再通过 Engine 创建有 Scope 的 Artifact；Download 先解析实体。数据库决定可见性与
策略，文件库负责字节且不暴露物理路径。

Provider 是编译期 Rust Adapter：

- `patchouli-provider`：公共边界；
- `patchouli-provider-sqlite`：本地持久化；
- `patchouli-provider-remote`：通过认证 HTTPS 传输 Provider Primitive；
- `patchouli-provider-router`：把规范 Scope JSON 映射到命名 Provider。

路由采用 First-match 和一个显式 Default，失败后不会切换 Provider；一个原子 Work
Unit 不能跨路由。`cluster_id` 与 `node_id` 标识服务进程，不代表已经实现复制。
IPC 在 macOS/Linux 使用 Unix Domain Socket，在 Windows 使用 Named Pipe，统一采用
UTF-8 NDJSON Frame。

## 模型接口

官方 Consumer 已提供自动检索和显式 Update/Retrieve Tool。自动检索默认开启，
Committed-turn 自动写入默认关闭。按 Turn 数、字符数和空闲时间触发仍属于 Consumer
后续工作，而不是统一服务或 Provider 的行为。

## 交付状态

已完成统一 Memory Service、响应式路由与持久化 Cursor、Agent Loop 适配器、
Harness-neutral 事务后端、本地/远程 Provider、可选 TypeScript 存储客户端、DSH
Artifact 摄取和两个 Indexer 包边界。尚待实现 Indexer 行为、MemoraX MemoryPlugin、
本地 Knowledge 抽取/检索插件，以及检查和重建等运维界面。

[产品源码](https://github.com/memorax-agent/dsh-patchouli/tree/main)维护在 `main`。
根包是 DSH 前端 Bundle，Rust 后端位于 `crates/`，`packages/protocol` 与 Harness
无关，其他 TypeScript 包是 DSH Adapter、Plugin 与 Indexer。
