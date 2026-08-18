# 快速开始

Patchouli 的两个部分既可单独使用，也可一起使用：

- DeepSeek Harness 插件 Bundle，对外提供统一 Memory Service；
- 可选的 `patchouli-db` 守护进程，提供本地或远程事务存储。

如果所有 MemoryPlugin 都使用自己的外部服务，则不需要数据库后端。

## 环境要求

- Node.js `^22.19.0` 或 `>=24.0.0`
- pnpm 11
- 从源码构建 `patchouli-db` 时需要 Rust stable
- 与 `0.1.0-rc.6` 兼容的 DeepSeek Harness

## 从源码安装 DSH 插件

```bash
git clone --branch main --single-branch https://github.com/memorax-ai/dsh-patchouli.git
cd dsh-patchouli
corepack enable
pnpm install
pnpm pack
dsh plugin --profile web add .
dsh --profile web --dump-config
```

Web Profile 中会列出 `patchouli`、Agent Loop 适配器、Artifact Ingestor、
Session/Workspace Indexer 和记忆游标服务。

## 安装可选后端

::: code-group

```bash [macOS / Linux]
curl -fsSL https://raw.githubusercontent.com/memorax-ai/dsh-patchouli/main/scripts/install.sh | sh
```

```powershell [Windows PowerShell]
irm https://raw.githubusercontent.com/memorax-ai/dsh-patchouli/main/scripts/install.ps1 | iex
```

:::

安装器会验证 Release 校验和并初始化 `~/.patchouli`，且不会覆盖已有配置。
它不会修改 DSH Profile、注册系统服务或启动后台进程。各平台细节与源码构建
方式参见[后端安装](./installation.md)。

## 配置存储客户端

默认 Bundle 会启用存储客户端。需要使用其他 Daemon 地址，或调整自动启动时，再改
它的配置：

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

`ctx.patchouliStorage` 提供守护进程控制、通用实体 CRUD、检索、Artifact 上传下载
和按游标读取变更。

## 下一步

- 在 [DSH 集成](./dsh-integration.md)中配置 Agent Loop Hook 与 Consumer。
- 阅读[架构](./architecture.md)，了解各进程和插件如何配合。
- 在[后端配置](./backend-configuration.md)中选择身份与一致性规则。
- 通过[知识模型](./knowledge-model.md)建模文件、知识与关系。
