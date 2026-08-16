<div align="center">
  <img width="100%" alt="Patchouli" src="assets/patchouli-banner-zh.png">
</div>

# dsh-patchouli

Patchouli 是面向 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 的本地知识依赖。目标是在不要求模型发起 Tool Call 的前提下，按当前任务检索相关知识，并通过 Harness 原生、可记录的上下文链路注入模型请求。

> 当前状态：仓库初始化阶段。现在提供可构建、可打包、可被 Cordis Loader 加载的最小插件骨架，尚未实现知识采集、索引和召回。

## 设计方向

Patchouli 将围绕三个边界逐步实现：

1. **Knowledge Service**：定义稳定的知识检索契约，供其他 Cordis 插件通过 `ctx` 使用。
2. **Provider**：负责本地知识的采集、索引和检索，不依赖 Agent 或 Prompt。
3. **Context Consumer**：在 `agent/pre-step` 阶段异步检索，并把带来源信息的有界结果追加到下一次模型请求。

默认模型界面是自动上下文注入，而不是知识库工具。详细约束见 [架构文档](docs/architecture.md)。

## 环境要求

- Node.js `^22.19.0` 或 `>=24.0.0`
- pnpm `11`
- DeepSeek Harness `0.1.0-rc.6` 或兼容版本

## 本地开发

```bash
pnpm install
pnpm check
```

构建产物位于 `lib/`。生成可安装的 npm tarball：

```bash
pnpm pack
```

在本地 DeepSeek Harness profile 中安装当前 checkout：

```bash
dsh plugin --profile web add .
dsh --profile web --dump-config
```

配置中应出现 `patchouli` 插件行。当前插件不会向模型注入任何内容。

## 仓库结构

```text
.
├── .github/workflows/ci.yml  # 验证与交付物打包
├── docs/                     # 架构和开发文档
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
