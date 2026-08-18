# 开发指南

产品代码与文档使用独立分支。`main` 保存 Monorepo、Runtime、协议、后端和测试；
`docs` 只保存 VitePress 站点。除非特别说明，本页命令均在 `main` 产品 Checkout 中运行。

## 产品环境

```bash
git clone --branch main --single-branch https://github.com/memorax-ai/dsh-patchouli.git
cd dsh-patchouli
corepack enable
pnpm install
pnpm check
cargo test --workspace
```

`pnpm check` 验证并测试全部 TypeScript Workspace，Rust 后端由 Cargo 独立检查。
CI 在 Ubuntu、macOS 和 Windows 上运行两套检查。`pnpm-workspace.yaml` 中的条目是
新发布 DSH 包绕过 pnpm Minimum-release-age 策略的显式例外。

## 运行 Daemon Shell

安装 Rust CLI 并初始化：

```bash
cargo install --path crates/server
patchouli-db init --root "$HOME/.patchouli"
```

默认布局使用 `~/.patchouli/data/patchouli.db`，从
`~/.patchouli/config.json` 加载策略。`init` 会创建缺失的策略、Provider 配置、
SQLite 目录、Artifact Store 和 Runtime 目录，不覆盖已有文件。

默认 Endpoint：

- macOS/Linux：`~/.patchouli/run/patchouli.sock`
- Windows：`\\.\pipe\patchouli`

macOS/Linux 生命周期：

```bash
patchouli-db serve \
  --endpoint "$HOME/.patchouli/run/patchouli.sock" \
  --artifacts "$HOME/.patchouli/data/artifacts" \
  --providers "$HOME/.patchouli/providers.json" \
  --config "$HOME/.patchouli/config.json"
patchouli-db status --endpoint "$HOME/.patchouli/run/patchouli.sock"
patchouli-db checkpoint --endpoint "$HOME/.patchouli/run/patchouli.sock"
patchouli-db stop --endpoint "$HOME/.patchouli/run/patchouli.sock"
```

PowerShell：

```powershell
$endpoint = '\\.\pipe\patchouli'
$config = Join-Path $HOME '.patchouli\config.json'
$providers = Join-Path $HOME '.patchouli\providers.json'
$artifacts = Join-Path $HOME '.patchouli\data\artifacts'
patchouli-db serve --endpoint $endpoint --artifacts $artifacts --providers $providers --config $config
patchouli-db status --endpoint $endpoint
patchouli-db checkpoint --endpoint $endpoint
patchouli-db stop --endpoint $endpoint
```

`serve` 保持前台运行，可交由 launchd、systemd、Windows Service Wrapper、容器或父
插件监管。启用 `autoStart` 的存储插件会以 Detached Process 启动同一命令。策略和
Provider Health Check 在监听 Socket 前完成。

默认策略支持事务 CRUD、通用检索、可保留变更流、持久跨 RPC Work Unit、Keyed
Idempotency 与持久 Causal/Session Frontier。Provider 能力不匹配会阻止启动。
Ctrl-C、Unix SIGTERM 与 Windows Console Shutdown 使用同一排空和关闭路径。

本地数据库只允许一个 Daemon 持有。启动会增加持久 Generation 并报告上次是否异常
结束；`checkpoint` 刷新 WAL 而不停机；`stop` 排空连接、标记 Clean、截断 WAL、关闭
SQLite 并释放锁。异常终止后 SQLite 会恢复已提交 WAL Transaction。

验证配置：

```bash
patchouli-db config check config/patchouli.default.json
patchouli-db config check config/patchouli.default.json --providers config/providers.local.json
```

运行真实第三方插件数据库回路：

```bash
pnpm test:e2e
```

该命令构建 Daemon、启动临时 SQLite，并挂载统一 Memory Service、存储客户端、
Artifact Ingestor 和私有 CRUD Test Plugin，验证直接调用与 DSH 来源的 Artifact
上下行及完整 CRUD。测试插件不属于默认 Bundle。

## 测试 DSH 前端

```bash
pnpm pack
dsh plugin --profile web add .
dsh --profile web --dump-config
```

默认 Bundle 包含：

- `patchouli`：注册 `ctx.patchouli`；
- `patchouli-storage`：注册 `ctx.patchouliStorage` 并连接 Daemon；
- `patchouli-agent-loop`：注册可配置 Hook 和两个模型 Tool；
- `patchouli-artifact-ingestor`：在存储、Attachment 与 FS 服务可用时摄取资源；
- `patchouli-session-indexer`：当前只声明 `patchouli + sessionQuery`；
- `patchouli-workspace-indexer`：当前只声明
  `patchouli + workspaceRegistry + fs`；
- `patchouli-cursors`：`storageDomain` 可用时注册 `ctx.patchouliCursors`。

Web Bundle 提供 `storageDomain`；Headless 默认不含存储栈，因此 Cursor Fiber Pending，
但 Memory Service、Agent Loop、Tool 和 Update/Retrieve 仍可加载。Headless Consumer
可加载 DSH 存储栈或传入自定义 `MemoryCursorStore`。

```ts
const cursorStore = ctx.patchouliCursors.bind({
  consumerId: 'example-consumer',
  subscriptionKey: 'memory-changes',
  scope,
})

const subscription = await ctx.patchouli.subscribe(
  {
    meta: {
      source: { type: 'consumer', id: 'example-consumer' },
      scope,
    },
  },
  change => applyChangeIdempotently(change),
  { cursorStore, signal, onError: reportSubscriptionError },
)
```

Binding Key 为 `(consumerId, subscriptionKey, scope, pluginId)`。服务先保存 Provider
Boundary，再串行处理单插件事件，并仅在 Handler 成功后推进 Cursor。只有标记为
Retryable 的 `MemorySubscriptionError` 会重连；Fatal、未知错误和 `resetRequired`
停止对应 Worker。Unsubscribe、Cordis Fiber 生命周期和 AbortSignal 都会取消重试并
排空已接收 Handler。

默认 Bundle 已启用存储客户端；本地存储型 MemoryPlugin 可配置 Endpoint 与自动启动：

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

低层客户端提供控制、CRUD、检索和 Cursor Subscription，并以 `PatchouliRpcError`
保留 Method、Code、Data 与 Reason。它不会等待或序列化 Handler；存储型 MemoryPlugin
负责映射连接丢失与 `CURSOR_EXPIRED`，高层服务负责 Cursor、去重、重试和生命周期。

## CI 与发布

CI 和发布都使用 GitHub-hosted Runner。Node 检查、Rust Lint、数据库 E2E 与三平台
Rust 测试会并行运行；汇总检查通过后，再并行构建 Linux x86_64/aarch64、macOS
x86_64/aarch64 和 Windows x86_64 的 Daemon。推送 `v*` Tag 会发布带 SHA-256 校验文件的
GitHub Release 和 npm 包。工作流不会安装或修改 DSH Profile，也不会部署到本地服务器。

## 文档站

编辑文档时使用独立 `docs` Checkout：

```bash
git clone --branch docs --single-branch \
  https://github.com/memorax-ai/dsh-patchouli.git dsh-patchouli-docs
cd dsh-patchouli-docs
corepack enable
pnpm install
pnpm docs:dev
```

开发服务器会在架构页面提供图表编辑器。点击 **编辑图表**，或直接打开
`/zh/architecture?edit-architecture=1`。选择桌面或移动布局后，可以编辑模块位置和
尺寸、Harness 外框、模块双语文案、单向边双语注释、路径归属，以及各布局的画布
高度。每条单向边还可以单独设置线型、箭头样式与大小和颜色，也可以在画布上直接
拖动模块。编辑器支持新增、删除模块与连线，切换模块是否位于 Harness 外框内，
修改连线端点和桌面/移动布局各自的 Handle，以及反转连线方向；删除模块时会一并
删除与它相连的边。点击 **保存到源码** 后，开发服务器会验证结果并原子更新
`docs/components/patchouli-architecture.data.json`；正式构建不包含编辑入口和写入接口。

提交前运行 `pnpm docs:build`。产品源码和产品构建命令有意不出现在该 Checkout 中。

## 生成文件

不要提交 `lib/`、Package Tarball、`target/`、Runtime Database 或本地 Harness State；
它们均由构建或运行时重新生成，并已加入忽略规则。
