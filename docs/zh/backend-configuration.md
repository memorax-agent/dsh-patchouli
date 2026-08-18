<script setup lang="ts">
import PatchouliConceptFlow from '../components/PatchouliConceptFlow.vue'
</script>

# 后端配置

数据库如何处理数据由部署配置决定，不写进 CRUD Wire Schema。除 Handshake 外，所有
协议消息都是 `{ meta, data }`：`data` 放方法本身的字段，后端按配置读取 `meta`。

策略文件不指定物理数据库。Adapter 和 Scope 路由使用单独的
[`config/providers.schema.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/providers.schema.json)。
唯一内置 SQLite Adapter 名为 `local`；远程条目包含 HTTPS Endpoint 和保存 Bearer
Token 的环境变量名。路由规则按顺序部分匹配规范 Scope，命中第一条，否则使用显式
Default；失败后不会回退到其他数据库。

前端插件不自己选择一致性或维护事务。它只能在 Metadata 中提供配置认识的字段；未提供
冲突策略时，后端使用该行为的默认值。

<ClientOnly>
  <PatchouliConceptFlow kind="backend" locale="zh" />
  <template #fallback>正在加载后端请求路径……</template>
</ClientOnly>

规范配置由 [`config/patchouli.schema.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patchouli.schema.json)
定义。单节点默认策略见 [`config/patchouli.default.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patchouli.default.json)，
共享事务示例见 [`config/patchouli.example.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patchouli.example.json)。

`retention.idempotency_seconds` 和 `retention.changes_seconds` 决定 Handshake
声明的持久重试与 Replay 保证。SQLite 在正常访问时清理过期记录。

内置 Fact Schema 通过稳定 URN 注册，并由实体策略引用：

```json
"knowledge": {
  "value_schema": { "$ref": "urn:patchouli:schema:knowledge:1" }
}
```

这样业务 Schema 和一致性配置彼此独立，其他实体类型也能复用同一种写法。

## 加载时检查

加载配置时，后端会在打开 Provider 或 Socket 前检查结构并生成行为计划。未知字段、
Alias 冲突、缺少身份键、不能同时使用的选项，或 Provider 能力不足，都会让启动失败。

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

## 字段怎么用

每个 Alias 可以用于不同地方：

- `scope_by`：形成授权与存储 Scope 前缀；
- `snapshot.key_by`：共享读取基线；
- `session.key_by`：持久化单调读、Read-your-writes 等会话 Frontier；
- `publication.key_by`：把多次 RPC 的 Staged Mutation 归入同一 Work Unit；
- `ordering.key_by`：确定线性化或串行 Commit Domain；
- `idempotency.key_by`：标识可重试 Mutation；
- `strategy_from`：读取请求级冲突策略。

同一个字段可以多次使用。Scope 总会加在前面，避免不同租户的数据混在一起；路由只会
匹配已经通过 `scope_by` 检查的字段。

## 实体策略与规则

`entities` 为每种 Entity Type 选择 Value Schema、默认冲突策略和字段 Merge Rule。
`behaviors` 定义一组命名行为；`rules` 按顺序选中其中一个。选中后，里面的要求要同时
满足，不会悄悄降低要求或改用其他行为。

行为可以组合：

- Snapshot Source 与 Freshness；
- Causal Token 和持久化 Session Frontier；
- Immediate 或 Marker Publication；
- Commit Ordering；
- Keyed Idempotency；
- Work Unit TTL 与过期策略。

不能同时满足的组合会在加载配置或规划请求时直接失败。

## 请求如何执行

Snapshot 决定请求读取哪个数据库时间点。Immediate Publication 在单次 RPC 内接受并
发布；Marker Publication 把 Mutation 持久化到 Work Unit，在 Closing Marker 到达时
一次性发布。跨 RPC 不会长期持有数据库事务。

Session 按配置的身份保存 Frontier。`monotonic_reads` 不让后一次读取回到更早的数据；
`read_your_writes` 要求看见这个 Session 已发布的写入。Opaque Causal Token 只有在配置
启用时才参与检查，客户端不能借它改用其他策略。

Provider 必须声明是否支持计划要求的 Authority/Replica、Snapshot、Frontier、Atomic Batch
等能力。Router 会在启动时检查每一条可能的路由，不等到第一条请求才发现问题。

## 常见一致性模式

### 默认单节点 SQLite

[`config/patchouli.default.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patchouli.default.json)
使用 Authority Snapshot、`workspace_id + user_id` Scope、线性化读取、Scope 内串行
Commit 和 Immediate Publication。Knowledge 默认在 `/content` 使用 Automerge，其他
字段使用 MVCC；KnowledgeRelation 默认使用 MVCC。它不启用 Session、Batch、Replica
或 Idempotency。`channel_id` 可用于 Session 控制，但不会按对话切碎长期知识。

### 最终一致多源读取

[`config/patterns/eventual.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patterns/eventual.json)
允许 Authority 或 Replica Snapshot，不要求 Freshness、Session 或 Commit Ordering，
并使用 MVCC 暴露并发候选。

### 因果插件会话

[`config/patterns/causal_session.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patterns/causal_session.json)
把可选 Opaque Causal Token 与持久化 `monotonic_reads`、`read_your_writes` 合并，
并在 Scope 内串行 Commit。Participant 必填，Token 可选。当前 SQLite 以 Authority
Frontier 执行；未来 Replica Provider 必须声明对应能力。

### 共享事务批次

[`config/patterns/shared_transaction.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patterns/shared_transaction.json)
用同一 Transaction Identity 绑定共享 Snapshot 与 Publication Key。首次请求记录全局
Change Cursor；后续首次访问的实体也按该 Cursor 重建基线。Mutation 持久化 Staging，
仅同一 Key 可见；Marker 在一个 SQLite 事务中发布全部实体和 Change Record。

首个请求固定事务级策略描述，后续请求必须选择相同 Snapshot、Publication、TTL 和
Ordering。示例使用过期丢弃且关闭幂等；完整选择规则见
[`config/patchouli.example.json`](https://github.com/memorax-ai/dsh-patchouli/blob/main/config/patchouli.example.json)。

## 事务如何划分

每个 CRUD Mutation 都在一个短数据库事务中记录不可变的 Candidate/Tombstone、已启用的
Idempotency 和 Causal/Group State，以及 Immediate 模式下的 Head 与 Change。成功响应
表示数据已经写入；Change Stream 表示它已经对订阅者可见。

Batch 会保存 Candidate 与 Group State，但不会在两次 RPC 之间一直占着数据库事务。
Closing Mutation 用一个短事务发布全部 Candidate。开始时记录的读取位置不会变化；如果
外部请求随后发布了同一实体，Engine 会按 Automerge/MVCC/Reject 处理，再用原子 CAS
发布。只有 `reject` 或最后一次 CAS 竞争会返回 `VERSION_CONFLICT`。超过 TTL 的 Work
Unit 会在 Provider 正常工作或执行生命周期操作时清理。

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
