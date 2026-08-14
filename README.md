# dsh-patchouli

Patchouli 是面向 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的本地知识依赖。目标是在不要求模型发起 Tool Call 的前提下，按当前任务检索相关知识，并通过 Harness 原生、可记录的上下文链路注入模型请求。

> 当前状态：已经具备可加载的 Cordis 插件、Rust `BackendEngine`、本地 IPC、控制 CLI、SQLite provider 生命周期、可配置的一致性/冲突计划，以及 `Knowledge`/`KnowledgeRelation` v1 类型和数据库条目。Knowledge 内容可使用 Automerge 合并，其他字段保留 MVCC 多版本。macOS/Linux 使用 Unix socket，Windows 使用 named pipe。CRUD 已经完成前后端接线，但尚未把存储执行逻辑接入上述计划；知识采集、索引和召回尚未实现。

daemon 会独占数据库实例，记录运行代次和正常/异常关闭状态。正常停止会先停止接收连接、关闭现有 IPC 会话，再执行 SQLite WAL checkpoint、持久化干净关闭标记并释放数据库锁；异常退出后的下一次启动由 SQLite WAL 自动恢复已提交事务，并在状态接口中报告恢复事实。

## 设计方向

Patchouli 将以 monorepo 形式围绕三个边界逐步实现：

1. **Knowledge Service**：定义稳定的知识检索契约，供其他 Cordis 插件通过 `ctx` 使用。
2. **Provider**：负责本地知识的采集、索引和检索，不依赖 Agent 或 Prompt。
3. **Context Consumer**：在 `agent/pre-step` 阶段异步检索，并把带来源信息的有界结果追加到下一次模型请求。

数据库后端使用 Rust 实现，独立于 DeepSeek Harness；TypeScript 仅承担插件和客户端协议类型。

默认模型界面是自动上下文注入，而不是知识库工具。详细约束见 [架构文档](docs/architecture.md)。
知识事实字段、关系方向和 SQLite 条目见 [事实模型](docs/knowledge-model.md)。

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
dsh plugin --profile web add .
dsh --profile web --dump-config
```

配置中应出现 `patchouli` 插件行。当前插件不会向模型注入任何内容。

先安装 daemon/CLI：

```bash
cargo install --path crates/server
```

将 backend policy 放到插件的默认配置位置：

```bash
mkdir -p "$HOME/.patchouli"
cp config/patchouli.default.json "$HOME/.patchouli/config.json"
```

插件加载时会连接默认本地 endpoint；若 daemon 不存在，则执行
`patchouli serve`，加载 `~/.patchouli/config.json` 并打开默认的
`~/.patchouli/data/patchouli.db` 后等待 handshake。插件卸载只关闭自身连接，daemon 由
`patchouli stop --endpoint <endpoint>` 显式停止。运行期间可用
`patchouli checkpoint --endpoint <endpoint>` 主动执行一次 WAL checkpoint。详细的三平台命令见
[开发文档](docs/development.md)。

## 仓库结构

```text
.
├── .github/workflows/ci.yml  # 验证与交付物打包
├── docs/                     # 架构和开发文档
├── crates/
│   ├── backend/              # Rust 数据库后端核心契约
│   ├── provider/             # 数据库 provider 公共边界
│   ├── provider-sqlite/      # 默认 SQLite adapter
│   └── server/               # 跨平台 daemon、IPC 和控制 CLI
├── packages/
│   └── protocol/             # 与 Harness 无关的数据库 JSON-RPC 契约
├── src/                      # Cordis 插件源码
├── test/                     # 最小契约测试
├── cordis.patch.yml          # DeepSeek Harness bundle 配置层
├── package.json
└── tsconfig.json
```

## CI 与交付

- Pull Request 和普通分支提交在 GitHub-hosted runner 上执行安装、类型检查、构建和测试。
- `main` 分支提交或手动运行工作流时，在仓库已注册的 `self-hosted` runner 上构建 npm tarball，并上传为 Actions artifact。
- 自动安装到服务器上的某个 DSH profile 尚未启用；需要先确定目标 profile、持久部署目录和回滚方式。

开发约定见 [docs/development.md](docs/development.md) 和 [CONTRIBUTING.md](CONTRIBUTING.md)。

## License

[MIT](LICENSE)
