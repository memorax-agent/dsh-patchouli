# @memorax-agent/dsh-patchouli-agent-loop

Cordis plugin adapting the official DeepSeek Harness Agent Loop to the
in-process `ctx.patchouliMemory` service. It owns the model tools and Agent Loop
hooks; it does not implement memory storage, indexing, extraction, or prompt
generation. Hook payloads are lossless JSON snapshots interpreted by registered
MemoryPlugins.

The default Patchouli DSH bundle loads this package automatically.

```yaml
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

| Direction | Data point | Delivery |
| --- | --- | --- |
| retrieve | `agent/session-start` | Best-effort `agent.inject()` because the official event is not awaited |
| retrieve | `agent/pre-step` | Awaited; recall JSON is appended to accepted step messages |
| retrieve | `agent/turn-stopping` | Awaited; recall JSON is passed to `agent.inject()` |
| retrieve | `tools/post-execute` | Awaited; recall JSON is appended to `additionalContexts` |
| update | `agent/created` | Per-Session update queue |
| update | `agent/disposed` | Per-Session update queue |
| update | `agent/request-error` | Per-Session update queue, without changing retry policy |
| update | `agent/error` | Per-Session update queue |
| update | `session/turn-end` | Complete committed turn event slice; enabled by default |
| update | `tools/result` | Frozen final Tool execution and result |

All calls use `{ meta, data }`. `meta.attributes.point` identifies the row,
while `data` contains only facts visible at that point. Updates are serialized
per Session, and the adapter extends `session/flush` to wait for admitted update
work.
