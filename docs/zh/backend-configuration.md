<script setup lang="ts">
import PatchouliConceptFlow from '../components/PatchouliConceptFlow.vue'
</script>

# 后端配置

数据库策略属于部署配置，不进入 CRUD Wire Schema。除 Handshake 外，协议消息统一为
`{ meta, data }`：`data` 保存方法业务字段，后端通过命名配置字段解释 `meta`。

策略文件不选择物理数据库。Adapter 和 Scope 路由使用独立的
[`config/providers.schema.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/providers.schema.json)。
唯一内置 SQLite Adapter 名为 `local`；远程条目包含 HTTPS Endpoint 和保存 Bearer
Token 的环境变量名。路由规则按顺序部分匹配规范 Scope，命中第一条，否则使用显式
Default；失败后不会回退到其他数据库。

前端插件不选择一致性，也不维护事务状态。它只能通过配置映射的 Metadata 字段请求
冲突策略；字段缺失时由选中的后端行为给出默认值。

<ClientOnly>
  <PatchouliConceptFlow kind="backend" locale="zh" />
  <template #fallback>正在加载后端请求路径……</template>
</ClientOnly>

规范配置由 [`config/patchouli.schema.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patchouli.schema.json)
定义。单节点默认策略见 [`config/patchouli.default.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patchouli.default.json)，
共享事务示例见 [`config/patchouli.example.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patchouli.example.json)。

`retention.idempotency_seconds` 和 `retention.changes_seconds` 决定 Handshake
声明的持久重试与 Replay 保证。SQLite 在正常访问时清理过期记录。

内置 Fact Schema 通过稳定 URN 注册，并由实体策略引用：

```json
"knowledge": {
  "value_schema": { "$ref": "urn:patchouli:schema:knowledge:1" }
}
```

因此业务 Schema 与一致性策略保持解耦，其他实体类型可复用相同策略结构。

## 验证

配置加载时即完成结构与语义验证。后端在打开 Provider 或 Socket 前编译行为计划；
未知字段、Alias 冲突、缺少身份键、非法策略组合或 Provider 能力不足都会阻止启动。

```bash
patchouli-db config check config/patchouli.default.json
patchouli-db config check config/patchouli.default.json \
  --providers config/providers.local.json
```

## Metadata 字段

`metadata.fields` 将 Wire Metadata 的物理键映射为后端逻辑 Alias：

```json
{
  "workspace_id": { "path": "meta.workspace_id", "required": true },
  "transaction_id": { "path": "meta.transaction_id", "required": false },
  "conflict_strategy": { "path": "meta.conflict_strategy", "required": false }
}
```

行为规则只引用 Alias，不硬编码前端键名。缺失可选字段表示该约束不参与本次请求；
缺失必填字段直接返回验证错误。

## Key 的职责

每个 Alias 可承担不同职责：

- `scope_by`：形成授权与存储 Scope 前缀；
- `snapshot.key_by`：共享读取基线；
- `session.key_by`：持久化单调读、Read-your-writes 等会话 Frontier；
- `publication.key_by`：把多次 RPC 的 Staged Mutation 归入同一 Work Unit；
- `ordering.key_by`：确定线性化或串行 Commit Domain；
- `idempotency.key_by`：标识可重试 Mutation；
- `strategy_from`：读取请求级冲突策略。

同一字段可以承担多个职责，但每项职责独立编译。Scope 始终作为隐式前缀，避免不同
租户碰撞；路由只能匹配 `scope_by` 中已经验证的字段。

## 实体策略与规则

`entities` 为每种 Entity Type 选择 Value Schema、默认冲突策略和字段级 Merge Rule。
`behaviors` 定义命名的一致性计划；`rules` 按顺序选择一个行为。规则是候选分支，
选中的行为内部约束取交集，不做降级或回退。

行为可以组合：

- Snapshot Source 与 Freshness；
- Causal Token 和持久化 Session Frontier；
- Immediate 或 Marker Publication；
- Commit Ordering；
- Keyed Idempotency；
- Work Unit TTL 与过期策略。

如果组合结果不可能满足，配置验证或请求规划会直接失败，而不是静默降低一致性。

## 执行语义

Snapshot 决定请求读取哪个数据库时间点。Immediate Publication 在单次 RPC 内接受并
发布；Marker Publication 把 Mutation 持久化到 Work Unit，在 Closing Marker 到达时
一次性发布。跨 RPC 不会长期持有数据库事务。

Session 约束通过配置身份保存 Frontier。`monotonic_reads` 防止后续读取倒退，
`read_your_writes` 要求看到本 Session 已发布写入。Opaque Causal Token 只在配置绑定
时参与约束，客户端不能借此选择其他策略。

Provider 必须声明计划所需的 Authority/Replica、Snapshot、Frontier、Atomic Batch
等能力。Router 会在启动时验证每条可能路由，而不是等第一条请求失败。

## 常见一致性模式

### 默认单节点 SQLite

[`config/patchouli.default.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patchouli.default.json)
使用 Authority Snapshot、`workspace_id + user_id` Scope、线性化读取、Scope 内串行
Commit 和 Immediate Publication。Knowledge 默认在 `/content` 使用 Automerge，其他
字段使用 MVCC；KnowledgeRelation 默认使用 MVCC。它不启用 Session、Batch、Replica
或 Idempotency。`channel_id` 可用于 Session 控制，但不会按对话切碎长期知识。

### 最终一致多源读取

[`config/patterns/eventual.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patterns/eventual.json)
允许 Authority 或 Replica Snapshot，不要求 Freshness、Session 或 Commit Ordering，
并使用 MVCC 暴露并发候选。

### 因果插件会话

[`config/patterns/causal_session.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patterns/causal_session.json)
把可选 Opaque Causal Token 与持久化 `monotonic_reads`、`read_your_writes` 合并，
并在 Scope 内串行 Commit。Participant 必填，Token 可选。当前 SQLite 以 Authority
Frontier 执行；未来 Replica Provider 必须声明对应能力。

### 共享事务批次

[`config/patterns/shared_transaction.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patterns/shared_transaction.json)
用同一 Transaction Identity 绑定共享 Snapshot 与 Publication Key。首次请求记录全局
Change Cursor；后续首次访问的实体也按该 Cursor 重建基线。Mutation 持久化 Staging，
仅同一 Key 可见；Marker 在一个 SQLite 事务中发布全部实体和 Change Record。

首个请求固定事务级策略描述，后续请求必须选择相同 Snapshot、Publication、TTL 和
Ordering。示例使用过期丢弃且关闭幂等；完整选择规则见
[`config/patchouli.example.json`](https://github.com/memorax-agent/dsh-patchouli/blob/main/config/patchouli.example.json)。

## 事务边界

每个 CRUD Mutation 使用一个短数据库事务，原子记录不可变 Candidate/Tombstone、
启用的 Idempotency 和 Causal/Group State，以及 Immediate 模式下的 Head 与 Change。
成功响应表示已持久接受；Change Stream 表示已经发布。

Batch 保存 Candidate 与 Group State，不在 RPC 之间保持数据库事务。Closing Mutation
通过一个短事务发布全部 Candidate。捕获的基线永不前移；若外部请求在此后发布同一
实体，Engine 按持久化的 Automerge/MVCC/Reject 策略协调，再用原子 CAS 发布。
只有 `reject` 或最终 CAS 竞争返回 `VERSION_CONFLICT`。超过 TTL 的 Work Unit 在
Provider 正常活动和生命周期操作中清理。

## 冲突处理

`strategy_from` 将物理请求键保持为部署配置。默认绑定 `meta.conflict_strategy`；省略
时使用 `default_strategy`。请求级覆盖只影响冲突处理，不改变一致性、幂等、Scope 或
Publication。

默认 Knowledge Merge Rule：

```json
{
  "path": "/content",
  "strategy": "automerge",
  "group_by": ["/kind"]
}
```

客户端仍提交完整 JSON Replacement。后端相对声明的 Base 计算 Diff，再合并并发分支；
String 使用协作文档，Map/List 使用 Automerge Object/Sequence。v1 的 List 按位置协调，
需要稳定元素身份时应建模为 Keyed Map。

`group_by` 防止文本与结构化内容被强制放入同一 CRDT。`otherwise: mvcc` 让不匹配值和
`/content` 外字段保留多 Head。相同最终版本会折叠；Metadata 不同则保留共享合并内容
的多个派生版本。SQLite 持久化物化 JSON、Automerge Change、依赖边和字段 Frontier；
Merge 创建派生版本，不修改旧版本。
