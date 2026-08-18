<script setup lang="ts">
import PatchouliConceptFlow from '../components/PatchouliConceptFlow.vue'
</script>

# Knowledge Fact 模型

Patchouli Fact IR v1 包含三种公开记录值：

- `artifact`，由 `urn:patchouli:schema:artifact:1` 验证；
- `knowledge`，由 `urn:patchouli:schema:knowledge:1` 验证；
- `knowledge_relation`，由 `urn:patchouli:schema:knowledge-relation:1` 验证。

三者复用通用实体 CRUD，不增加 Knowledge 专用 JSON-RPC。规范 Schema 与示例位于
产品分支的 [`packages/protocol/schemas`](https://github.com/memorax-ai/dsh-patchouli/tree/main/packages/protocol/schemas)。

## 身份

实体身份与 Fact 值有意分离：

<ClientOnly>
  <PatchouliConceptFlow kind="identity" locale="zh" />
  <template #fallback>正在加载实体身份关系图……</template>
</ClientOnly>

通用实体 Envelope 保存实体 ID 和存储版本，Fact 值不会在 `metadata` 中重复
`id` 或 `revision`。请求 `meta` 经配置生成 `scope_json`，每个数据库键都会带上它，
用来确定访问范围和存放位置。`metadata.core.scope` 只描述租户、工作区、用户和
Session；它来自不可信输入，不能扩大配置指定的 Scope。

## Artifact

文件、向量和其他非 JSON 资源都是一等 `artifact` 实体。Knowledge 不嵌入字节，也不
复制它们的存放信息。每个 Artifact 包含媒体类型、可选名称、可选长度和摘要、描述性
元数据，以及一种 Placement：

```text
managed  Patchouli 在 provider + key 处拥有字节
indexed  外部 Provider 在 provider + locator + revision 处拥有字节
```

本地文件和远程对象 ID 都使用相同的 `indexed` 写法。Provider 解释不透明 Locator，
通用 CRUD 与 Knowledge Consumer 不区分本地或远程。Locator 和 Key 不得包含凭据。

`managed` Artifact 必须同时具有 `byte_length` 和 `digest`；`indexed` Artifact
在外部 Provider 只暴露 Revision 时可以缺少两者。修改外部 Revision 或将其提升为
托管存储都会创建正常的新实体版本。

托管字节通过 Artifact Upload RPC 进入后端。Commit 会验证长度和 SHA-256、对相同内容
去重，再按 Scope、事务、一致性和冲突规则创建实体并发布变更。下载先查带 Scope 的
实体，客户端不会获得后端文件路径。Placement Provider 是保存字节的 Daemon Node ID，
其他节点可以拒绝发错目标的请求。

`indexed` Artifact 使用通用 CRUD，由 Placement 指定的 Provider 解释 Locator。
删除或替换实体不会立刻删除托管字节，因为内容寻址对象可能被共享。未完成上传在
重启时丢弃；v1 尚未实现孤儿回收和跨节点字节复制。

Knowledge 通过 `{ type: "artifact", id, role }` 引用 Artifact。`role` 可为
`source`、`attachment` 或 `embedding`；媒体类型、摘要、Placement 与 Provenance
只保存在 Artifact 实体中。

## Knowledge

每个 `KnowledgeValue` 有四个必填字段：

```text
content   文本或结构化 JSON
metadata  固定 Core 与带命名空间的扩展
artifact  零个或多个带类型的 Artifact 引用
profile   七组描述字段
```

`content` 是 `{ kind: "text", text }` 或 `{ kind: "structured", value }`。
二进制和向量不会嵌入 Content。

`metadata.core` 固定 Schema 身份、Scope、来源、时间戳、生命周期与 Provenance。
所有可空 Core 字段都显式保留为 `null`，避免将省略误解为未知。扩展键至少包含一个
命名空间分隔符，例如 `local.session`。Embedding 模型和维度等表示层信息属于
Artifact Metadata 扩展，不复制到每个引用。

Profile 包含以下字段：

| 字段 | 取值 |
| --- | --- |
| epistemic | `unknown`、`observation`、`hypothesis`、`belief`、`knowledge`、`derived` |
| temporal | `unknown`、`timeless`、`instant`、`interval`、`sequence` |
| ownership | `unknown`、`world`、`agent`、`user`、`shared` |
| abstraction | `unknown`、`instance`、`pattern`、`concept`、`rule` |
| persistence | `unknown`、`working`、`short_term`、`long_term`、`permanent` |
| retrieval | 一个或多个 `unknown`、`exact`、`associative`、`contextual`、`causal`、`procedural` |
| actionability | `unknown`、`informational`、`directive`、`constraint` |

Retrieval 描述认知行为，而不是全文、向量或 Trigram 等查询实现。Actionability
也只是描述；存储为 `directive` 或 `constraint` 不会授予执行权限。

## KnowledgeRelation

Relation 值包含固定 Relation Type、非空 `from` 集合、非空 `to` 集合和具有独立
Schema 身份的同构 Metadata。每个集合内的引用必须唯一。v1 类型如下：

| 类型 | 方向 |
| --- | --- |
| `supports` | 支持知识集合 → 被支持知识集合 |
| `contradicts` | 冲突知识集合 → 被反驳知识集合 |
| `derived_from` | 派生知识集合 → 来源知识集合 |
| `generalized_from` | 泛化知识集合 → 来源实例集合 |
| `causes` | 原因知识集合 → 结果知识集合 |
| `supersedes` | 替代知识集合 → 旧知识集合 |

所有端点都在 Relation 实体的配置存储 Scope 中解析；v1 不支持跨 Scope Relation。
更新可以同时替换 `type`、`from`、`to` 和 `metadata`，并创建新的不透明版本。
两个集合可以重叠，因此自关系和环都是合法记录；后端不限制图的拓扑。

JSON Schema 验证本地结构。Controller 还会检查端点存在性、共同 Scope、Tombstone
和通用版本/冲突策略，但不会检查环，也不会强制保留旧 Relation 的类型或端点。

## SQLite 条目

SQLite Schema v11 定义两个主要表：

- `patchouli_entity_version`：按规范化
  `scope_json + entity_type + entity_id + version` 保存不可变活动值与 Tombstone；
- `patchouli_entity_head`：保存当前发布的 Head 集合，通常一个，`mvcc` 下可有多个。

`patchouli_crdt_change`、`patchouli_crdt_change_parent` 和
`patchouli_entity_crdt_head` 保存 Automerge Change、依赖图和字段 Frontier；
`patchouli_change` 在同一事务中记录每次已提交 Head 转换，用于响应式投递。

每个已发布版本记录可见时的 Change Cursor。`patchouli_work_unit*` 通过该 Cursor
跨 RPC 重建固定数据库基线，同时在 Marker Close 前阻止 Staged Version 进入已发布
Head 和类型化 View。

活动行必须包含有效 JSON，Tombstone 的值必须为 `null`。值只存储一次；
`patchouli_artifact`、`patchouli_knowledge` 和 `patchouli_knowledge_relation`
是活动 Head 上的只读 View，不会成为第二份数据来源。旧存储 Schema 会被明确拒绝，
当前不提供迁移或兼容方案。
