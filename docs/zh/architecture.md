<script setup lang="ts">
import PatchouliArchitecture from '../components/PatchouliArchitecture.vue'
import PatchouliConceptFlow from '../components/PatchouliConceptFlow.vue'
</script>

# 架构

## 已有部分

Patchouli 目前有两部分可以使用：

- 与 Harness 无关的 Rust 后端：事务 CRUD、类型化知识、Automerge/MVCC、词法检索、
  可保留的变更流、SQLite、认证远程 Provider、按 Scope 选择 Provider，以及 Daemon
  启动恢复；
- DSH 的统一 Memory Service：响应式订阅、持久化的 Consumer Cursor、官方 Agent Loop
  适配器、托管 Artifact 摄取，以及 Session/Workspace Indexer 包。

Session 和 Workspace Indexer 还没有实际扫描逻辑，通用的 Knowledge 抽取与检索插件
也还在计划中。Bundle 里的 TypeScript 存储客户端已经能控制后端、执行 CRUD 和检索、
传输 Artifact、处理结构化 RPC 错误，并按 Cursor 订阅变更。

## 目标

让 DSH 有一个统一的记忆入口，同时不把 Agent Loop 的调用时机、插件算法和存储实现
绑在一起。

<ClientOnly>
  <PatchouliArchitecture locale="zh" />
  <template #fallback>正在加载架构图……</template>
</ClientOnly>

`ctx.patchouli` 是 DSH 进程内服务，不通过外部 Bridge 暴露。其他 Harness 和
外部应用需要自己的适配器。

## 各部分做什么

### 统一 Memory Service

根插件 `dsh-patchouli` 注册 `ctx.patchouli`，负责稳定的
`update` / `retrieve` / `subscribe` 接口、MemoryPlugin 注册、路由和来源标记。
它不做存储，也不决定 Agent 或 Prompt 的行为。

每次调用都带有可信的 Source Identity、Opaque Scope、可选 JSON Attributes 和操作
参数。插件可以注册同步 Filter；没有 Filter 时会收到所有调用。某个 Filter 抛出异常，
只会让该插件这一次路由失败。服务把结果和来源一起返回，并分别记录错误；不会解释
插件的 `data`，也不会把模型 Schema 暴露出去。

### MemoryPlugin

MemoryPlugin 实现高层 `update` 与 `retrieve`，也可以实现 `subscribe`。`update` 是把
信息交给插件处理，不等于替换一个数据库实体。调用方不能指定插件 ID；插件用自己的
Filter 表明应该接收哪些调用。

远程插件可以直接调用外部 API，本地插件可以使用存储客户端。存储层的 CRUD 类型不在
统一 Memory Service 的接口里。

Artifact Ingestor 只处理 Update，把 DSH 资源引用变成托管 Artifact。Session 图片由
`ctx.attachments` 读取，`workspace-file` 由 `ctx.fs` 读取并确认仍在工作区内，随后通过
`ctx.patchouliStorage.uploadArtifact` 上传。协调服务、存储客户端、Attachment 或 FS
服务缺失时，对应 Cordis Fiber 会保持 Pending。

每个插件按自己的顺序处理订阅事件，并在 Handler 成功后保存 Cursor。可重试错误会以
250 ms 到 30 秒的带抖动指数退避，从最后保存的 Cursor 重新连接；Fatal、未知错误和
`resetRequired` 只会停掉对应插件。需要重置时，Consumer 要自己完成快照、替换订阅并
删除该插件的 Cursor。`unsubscribe` 会取消重试、关闭 Provider Handle，并处理完已收到
的任务。

### Agent Loop Consumer

Agent Loop 连接器把官方 Agent、Session、Tool 扩展点映射到
统一服务，并注册 `memory_update` 与 `memory_retrieve`。检索点包括
`agent/session-start`、`agent/pre-step`、`agent/turn-stopping`、
`tools/post-execute`；存储点包括 Agent 生命周期与错误、已提交 `session/turn-end`
和 `tools/result`。默认只启用 `agent/pre-step`、`session/turn-end` 和两个模型工具。

适配器发送 Hook 点位能看到的原始 JSON，不生成检索 Prompt，不总结，也不判断哪些
内容算作记忆。检索成功后，每个插件的结果都会成为一条带来源的 JSON Recall Message，
再通过官方上下文通道注入。`session-start` 是 Fire-and-forget；其他等待型 Hook 仍按
官方行为等待完成。

每次调用都带有规范化的 `meta`，Source 固定为
`{ type: "agent-loop", id: "dsh-patchouli-agent-loop" }`。Scope 来自 Session
工作目录，缺失时回退到 Session ID；Attributes 记录 Point、Session 和位置。模型
不能伪造这个可信 Envelope。

### Indexer 包

Session Indexer 依赖 `ctx.patchouli + ctx.sessionQuery`，未来负责 Session 扫描
和增量提交。Workspace Indexer 依赖
`ctx.patchouli + ctx.workspaceRegistry + ctx.fs`，未来负责工作区遍历和变更
观察。目前两者只定义了包和依赖；依赖缺失时保持 Pending，不改用其他数据源。

### 持久化 Consumer Cursor

`dsh-patchouli/cursor-store` 注册 `ctx.patchouliCursors`。`bind` 使用
`consumerId + subscriptionKey + scope`，再加上 Plugin ID 区分游标。服务只保存 Opaque
Cursor；快照和重置仍由 Consumer 处理。

Web Assembly 提供 `storageDomain`，Cursor 会保存到 DSH 存储。Headless 没有存储服务时，
只有 Cursor Fiber 会 Pending；Memory Service、Agent Loop 和 Update/Retrieve 仍可使用。
Headless Consumer 也可以直接提供其他 `MemoryCursorStore`。

### 存储客户端

`dsh-patchouli/storage` 注册 `ctx.patchouliStorage`，连接已有 Daemon；Endpoint 不可用时
也可启动一个。默认 Bundle 已包含该客户端。部署只使用远程 MemoryPlugin 时，可以禁用
这一项。

客户端提供状态、Checkpoint、通用实体 CRUD/检索、托管 Artifact 上传下载和变更订阅。
`PatchouliRpcError` 会保留 Method、Code、Data 和可选 Reason。这个低层客户端只按 Wire
顺序分发请求，不替异步 Handler 排队、不保存 Cursor、也不负责重连；存储型 MemoryPlugin
负责把这些能力接到高层订阅。卸载插件只会关闭 IPC，不会停止独立 Daemon。

### 存储后端

Rust 后端拥有持久化、通用实体 CRUD、检索和变更流，不依赖 Agent 生命周期、Prompt、
DSH 或 Cordis。首个 Fact Vocabulary 包含 `artifact`、`knowledge` 和
`knowledge_relation`，详见[知识模型](knowledge-model.md)。

<ClientOnly>
  <PatchouliConceptFlow kind="backend" locale="zh" />
  <template #fallback>正在加载后端请求路径……</template>
</ClientOnly>

Controller 负责 Schema、身份提取、一致性计划、Work Unit、冲突、幂等和发布。
`BackendEngine` 持有已经验证的配置和一个 Provider，不在内存里保存跨请求的 Transaction
Map。Provider 管理持久化：启动时取得独占锁并恢复 SQLite，关闭时处理完连接、标记为
Clean、Checkpoint WAL 并释放锁。Mutation 写入成功后才会返回。

Daemon 的 Artifact Store 是独立的本地内容寻址文件库。Upload Commit 会验证并保存字节，
再由 Engine 创建带 Scope 的 Artifact；Download 会先查实体。数据库决定谁能看到它和
如何处理版本，文件库只管理字节，不暴露物理路径。

Provider 是编译期 Rust Adapter：

- `patchouli-provider`：公共接口；
- `patchouli-provider-sqlite`：本地持久化；
- `patchouli-provider-remote`：通过认证 HTTPS 传输 Provider Primitive；
- `patchouli-provider-router`：把规范 Scope JSON 映射到命名 Provider。

路由按顺序匹配，最后使用明确的 Default。匹配的 Provider 失败时不会换一个继续尝试；
一个原子 Work Unit 也不会跨 Provider。`cluster_id` 与 `node_id` 只是服务进程的标识，
不表示已经具备复制能力。IPC 在 macOS/Linux 使用 Unix Domain Socket，在 Windows 使用
Named Pipe，消息统一为 UTF-8 NDJSON Frame。

## 模型接口

官方 Consumer 已提供自动检索和显式 Update/Retrieve Tool。自动检索默认开启，
Committed-turn 自动写入默认关闭。按 Turn 数、字符数和空闲时间触发仍属于 Consumer
后续工作，而不是统一服务或 Provider 的行为。

## 还在进行的工作

统一 Memory Service、响应式路由与持久化 Cursor、Agent Loop 适配器、与 Harness 无关的
事务后端、本地/远程 Provider、Bundle 内的 TypeScript 存储客户端、DSH Artifact 摄取和
两个 Indexer 包都已就位。接下来会实现 Indexer 的实际行为、MemoraX MemoryPlugin、
本地 Knowledge 抽取/检索插件，以及检查和重建等运维界面。

[产品源码](https://github.com/memorax-ai/dsh-patchouli/tree/main)维护在 `main`。
根包是 DSH 前端 Bundle，Rust 后端位于 `crates/`，`packages/protocol` 与 Harness
无关，其他 TypeScript 包是 DSH Adapter、Plugin 与 Indexer。
