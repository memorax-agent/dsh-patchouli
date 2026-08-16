# DeepSeek Harness integration

Patchouli's high-level API is an in-process DSH service. External applications
and other harnesses require their own adapters; Patchouli does not expose the
Memory Service over an external bridge.

## Common Memory Service

`ctx.patchouliMemory` exposes three operations:

- `update` submits information for registered plugins to absorb;
- `retrieve` asks registered plugins for relevant information;
- `subscribe` streams plugin-owned changes to a reactive consumer.

Calls contain `meta` plus plugin-defined JSON `data`. The service routes each
call through filters supplied when plugins register, then returns one outcome
per matching plugin. Callers do not select internal plugins directly.

## Agent Loop adapter

The official adapter sends the complete data observable at each enabled hook.
It does not add retrieval prompts, summarize events, or decide how a plugin
should remember them.

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

The default policy retrieves before each agent step, stores every committed
turn, and exposes `memory_retrieve` and `memory_update` to the model. Other hook
points are opt-in. `agent/session-start` is a non-waiting notification, so its
retrieval path is best-effort; waiting hooks preserve the official Agent Loop
semantics.

`memory_update` can submit workspace files without reading bytes into the model
call:

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

The Artifact Ingestor resolves the path inside the active Session workspace and
uploads the bytes through the optional storage client. Image attachments follow
the same managed Artifact path.

## Reactive consumers

Bind a durable cursor namespace before subscribing:

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

Each plugin is processed serially and advances its cursor only after the handler
succeeds. Plugins do not block one another. Retryable classified failures resume
from the last durable cursor; unknown failures and `resetRequired` stop that
plugin worker so the consumer can perform an explicit snapshot or resync.

`subscription.unsubscribe()` is idempotent, cancels retries, closes underlying
subscriptions, and waits for admitted handlers to finish. Handlers must remain
idempotent because a process can stop after applying a side effect but before
persisting its cursor.
