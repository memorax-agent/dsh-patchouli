# 后端安装

Patchouli 通过一个 `patchouli-db` 可执行文件提供守护进程、控制 CLI、SQLite
Provider、远程 Provider Server 和初始配置模板。安装过程不会修改 Harness
Profile、注册系统服务或启动后台进程。

## 支持的 Release 产物

| 平台 | 架构 | Release 文件 |
| --- | --- | --- |
| Linux | x86_64 | `patchouli-db-linux-x86_64` |
| Linux | aarch64 | `patchouli-db-linux-aarch64` |
| macOS | Intel | `patchouli-db-macos-x86_64` |
| macOS | Apple Silicon | `patchouli-db-macos-aarch64` |
| Windows | x86_64 | `patchouli-db-windows-x86_64.exe` |

每个文件都有对应的 `.sha256`。安装器会同时下载并校验两者，验证失败时不会安装。
Linux 产物使用静态 musl 运行时，不依赖宿主发行版的 glibc 版本。

## 安装 Release

macOS 或 Linux：

```bash
curl -fsSL https://raw.githubusercontent.com/memorax-ai/dsh-patchouli/main/scripts/install.sh | sh
```

默认安装到 `~/.local/bin/patchouli-db`，必要时请将该目录加入 `PATH`。

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/memorax-ai/dsh-patchouli/main/scripts/install.ps1 | iex
```

默认安装到 `%LOCALAPPDATA%\Patchouli\bin\patchouli-db.exe`，必要时加入 `PATH`。

两个安装器都会初始化 `~/.patchouli`，保留并验证已有配置而不覆盖。可选环境变量：

- `PATCHOULI_VERSION`：Release Tag，例如 `v0.1.0`；默认使用最新版；
- `PATCHOULI_INSTALL_DIR`：二进制目标目录；
- `PATCHOULI_HOME`：后端数据与配置目录。

重复运行安装器即可升级。Windows 上需先停止守护进程，因为系统可能锁定可执行文件。
Unix 新建目录使用 `0700`，配置和数据库文件使用 `0600`；如果已有受管路径允许组或
其他用户访问，安装会拒绝继续并报告路径。升级时会先用新二进制验证配置。

## 从源码安装

需要 Rust stable 和 C 工具链：

```bash
cargo install --locked --git https://github.com/memorax-ai/dsh-patchouli \
  --package patchouli-server
patchouli-db init --root "$HOME/.patchouli"
```

若使用本地 `main` Checkout，请在仓库根目录将第一条命令替换为
`cargo install --locked --path crates/server`。

## 初始化目录

`patchouli-db init --root <path>` 会创建并验证：

```text
<path>/
├── config.json              # 后端策略
├── providers.json           # 本地/远程 Provider 路由
├── patchouli.schema.json    # config.json 的 Schema
├── providers.schema.json    # providers.json 的 Schema
├── data/                    # 默认 SQLite 数据目录
│   └── artifacts/           # 后端管理的内容寻址文件
└── run/                     # Unix Socket 目录
```

命令只创建缺失文件；已有文件无效时会报告错误并保持原样。

在前台启动本地后端：

```bash
patchouli-db serve \
  --endpoint "$HOME/.patchouli/run/patchouli.sock" \
  --artifacts "$HOME/.patchouli/data/artifacts" \
  --providers "$HOME/.patchouli/providers.json" \
  --config "$HOME/.patchouli/config.json"
```

Windows 使用 `\\.\pipe\patchouli`。进程监管由部署环境决定；`serve` 可运行在
launchd、systemd、Windows Service Wrapper 或父插件进程下。

## 卸载

停止守护进程后只删除安装的可执行文件。安装器不会删除 `PATCHOULI_HOME`；配置和
SQLite 数据会一直保留，直到用户主动删除该目录。
