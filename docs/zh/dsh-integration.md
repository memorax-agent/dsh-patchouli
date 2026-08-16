# DeepSeek Harness 集成

Patchouli 的高层 API 是 DSH 进程内服务。外部应用和其他 Harness 需要单独的
适配器；Patchouli 不通过外部 Bridge 暴露 Memory Service。

## 统一 Memory Service

`ctx.patchouliMemory` 提供三种操作：

- `update`：向注册插件提交待吸收的信息；
- `retrieve`：向注册插件查询相关信息；
- `subscribe`：将插件拥有的变更推送给响应式 Consumer。

每次调用包含 `meta` 和由插件定义的 JSON `data`。服务根据插件注册时提供的
过滤器路由调用，并为每个匹配插件返回独立结果。调用方不能直接选择内部插件。

## Agent Loop 适配器

官方适配器会发送每个已启用 Hook 点位能够观察到的完整数据。它不会添加检索
提示词、总结事件或决定插件应如何形成记忆。

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

默认策略在每个 Agent Step 前检索、保存每个已提交 Turn，并向模型暴露
`memory_retrieve` 和 `memory_update`。其他 Hook 点位需显式开启。
`agent/session-start` 是非等待通知，因此该点的检索采用尽力而为；其他等待型
Hook 保留官方 Agent Loop 的同步语义。

`memory_update` 可以提交工作区文件，而不必先把文件字节读入模型调用：

```json
{
  "resources": [
    {
      "kind": "workspace-file",
      "path": "notes/decision.md",
      "mediaType": "text/markdown",
      "role": "reference"
    }
  ]
}
```

Artifact Ingestor 在当前 Session 工作区内解析路径，并通过可选存储客户端上传
字节；图片附件也经过同一条托管 Artifact 链路。

## 响应式 Consumer

订阅前先绑定持久化游标命名空间：

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

每个插件按顺序处理事件，只有 Handler 成功后才推进游标；不同插件互不阻塞。
标记为可重试的失败从最后一个持久化游标恢复，未知失败和 `resetRequired` 会停止
对应插件 Worker，交由 Consumer 显式执行快照或重新同步。

`subscription.unsubscribe()` 可重复调用，会取消重试、关闭底层订阅并等待已接收
的 Handler 完成。Handler 必须保持幂等，因为进程可能在副作用完成、游标写入前停止。
