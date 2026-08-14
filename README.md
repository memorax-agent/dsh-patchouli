# dsh-patchouli

Patchouli 是面向 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的本地知识依赖。目标是在不要求模型发起 Tool Call 的前提下，按当前任务检索相关知识，并通过 Harness 原生、可记录的上下文链路注入模型请求。

> 当前状态：仓库初始化阶段。现在提供可构建、可打包、可被 Cordis Loader 加载的最小插件骨架，尚未实现知识采集、索引和召回。

## 设计方向

Patchouli 将以 monorepo 形式围绕三个边界逐步实现：

1. **Knowledge Service**：定义稳定的知识检索契约，供其他 Cordis 插件通过 `ctx` 使用。
2. **Provider**：负责本地知识的采集、索引和检索，不依赖 Agent 或 Prompt。
3. **Context Consumer**：在 `agent/pre-step` 阶段异步检索，并把带来源信息的有界结果追加到下一次模型请求。

数据库后端使用 Rust 实现，独立于 DeepSeek Harness；TypeScript 仅承担插件和客户端协议类型。

默认模型界面是自动上下文注入，而不是知识库工具。详细约束见 [架构文档](docs/architecture.md)。

## 环境要求

- Node.js `^22.19.0` 或 `>=24.0.0`
- pnpm `11`
- Rust stable
- DeepSeek Harness `0.1.0-rc.6` 或兼容版本

## 本地开发

```bash
pnpm install
pnpm check
cargo test --workspace
```

根插件和各 package 的构建产物位于各自的 `lib/`。生成可安装的 npm tarball：

```bash
pnpm pack
```

在本地 DeepSeek Harness profile 中安装当前 checkout：

```bash
dsh plugin --profile web add github:CH4ACKO3/dsh-ui-container
dsh plugin --profile web add github:CH4ACKO3/dsh-ui-workspace
dsh plugin --profile web add .
dsh --profile web --dump-config
```

三个客户端 bundle 都是运行时必需项，必须安装在同一个 profile；缺少 Container 或
Workspace 时 Patchouli 会拒绝安装或保持未激活。配置中应各出现一个对应插件行。
当前插件不会向模型注入任何内容。

浏览器端分为 UI 容器、Workspace 预设和记忆前端。容器通过 Cordis 的
`uiContainer` service 接受多个具名 surface；每个前端取得隔离的
`UiSurfaceConnection`，共享文档 provider 与响应式数据源，但分别维护入口、布局和会话 Host。
同一 package 还提供可递归嵌套的 `SurfaceHost` 视觉边界；子 Host 默认继承父级的
surface 与 session，也可以切换到另一条连接。该组件只在浏览器本地建立 React 上下文和 DOM
边界，不预渲染或远程传输组件树。
容器同时提供 MessagePort 与 WebSocket 远程通道：跨进程或远程前端只接收 URI 文档投影与
变更失效通知，仍在本地用 Workspace 渲染；相同 revision 不会重复发送正文。远程页面控制
能力默认关闭，WebSocket 的认证、授权与 TLS 由建立端点的宿主负责。接入方式见
[UI Container remote transport](https://github.com/CH4ACKO3/dsh-ui-container#remote-transport)。
Workspace 提供 Explorer、可拖动 Pane、标签页、文档页面和动作栏等可选模板。
Patchouli 以 `patchouli.memory` surface 接入，用 Workspace 模板组装页面，并注册 Harness
原生的“知识”会话视图。

客户端知识视图默认继承 Harness 的明暗主题和当前语言，并导出
`patchouliTheme.set({ browse, edit })` 供宿主分别配置浏览与编辑模式的颜色、阴影、字体和动效；调用
`patchouliTheme.reset()` 即恢复继承。Explorer 栏目通过 `ctx.patchouliMemoryUi.explorerPanes`
注册，文档动作和过滤器也由同一个贡献 service 暴露。具体接口与示例见
[UI 设计文档](docs/ui-design.md)。

## 仓库结构

```text
.
├── .github/workflows/ci.yml  # 验证与交付物打包
├── docs/                     # 架构和开发文档
├── crates/
│   └── backend/              # Rust 数据库后端核心
├── packages/
│   ├── memory-ui/            # Patchouli 记忆前端与 Harness 会话视图
│   └── protocol/             # 与 Harness 无关的数据库 JSON-RPC 契约
├── src/                      # Cordis 插件源码
├── test/                     # 最小契约测试
├── cordis.patch.yml          # DeepSeek Harness bundle 配置层
├── package.json
└── tsconfig.json
```

通用 UI 基础设施已拆分为独立仓库：

- [dsh-ui-container](https://github.com/CH4ACKO3/dsh-ui-container)：可递归视觉容器、文档投影与远程通道。
- [dsh-ui-workspace](https://github.com/CH4ACKO3/dsh-ui-workspace)：Explorer、标签页和文档表面等预设组件。

## CI 与交付

- Pull Request 和普通分支提交在 GitHub-hosted runner 上执行安装、类型检查、构建和测试。
- `main` 分支提交或手动运行工作流时，在仓库已注册的 `self-hosted` runner 上构建 npm tarball，并上传为 Actions artifact。
- 自动安装到服务器上的某个 DSH profile 尚未启用；需要先确定目标 profile、持久部署目录和回滚方式。

开发约定见 [docs/development.md](docs/development.md) 和 [CONTRIBUTING.md](CONTRIBUTING.md)。

## License

[MIT](LICENSE)
