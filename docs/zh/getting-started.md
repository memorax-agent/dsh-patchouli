# 快速开始

Patchouli 提供两个可以独立使用的部分：

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
git clone --branch main --single-branch https://github.com/memorax-agent/dsh-patchouli.git
cd dsh-patchouli
corepack enable
pnpm install
pnpm pack
dsh plugin --profile web add .
dsh --profile web --dump-config
```

Web Profile 中应出现核心 `patchouli` 插件、Agent Loop 适配器、Artifact
Ingestor、Session/Workspace Indexer 边界以及持久化记忆游标服务。

## 安装可选后端

::: code-group

```bash [macOS / Linux]
curl -fsSL https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.sh | sh
```

```powershell [Windows PowerShell]
irm https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.ps1 | iex
```

:::

安装器会验证 Release 校验和并初始化 `~/.patchouli`，且不会覆盖已有配置。
它不会修改 DSH Profile、注册系统服务或启动后台进程。各平台细节与源码构建
方式参见[后端安装](./installation.md)。

## 启用存储客户端

存储客户端不属于默认 Bundle。只有需要 Patchouli 守护进程的插件才应启用：

```yaml
- id: patchouli-storage
  name: dsh-patchouli/storage
  config:
    autoStart: true
```

客户端通过 `ctx.patchouli` 提供守护进程控制、通用实体 CRUD、检索、托管
Artifact 传输和基于游标的变更订阅。

## 下一步

- 在 [DSH 集成](./dsh-integration.md)中配置 Agent Loop Hook 与 Consumer。
- 阅读[架构](./architecture.md)，了解进程和插件边界。
- 在[后端配置](./backend-configuration.md)中选择身份与一致性规则。
- 通过[知识模型](./knowledge-model.md)建模文件、知识与关系。
