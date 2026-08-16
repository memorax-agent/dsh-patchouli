# Backend installation

Patchouli ships one `patchouli-db` executable containing the daemon, control CLI,
SQLite provider, remote-provider server, and initial configuration templates.
Installation does not modify a Harness profile, register a system service, or
start a background process.

## Supported release assets

| Platform | Architecture | Release asset |
| --- | --- | --- |
| Linux | x86_64 | `patchouli-db-linux-x86_64` |
| Linux | aarch64 | `patchouli-db-linux-aarch64` |
| macOS | Intel | `patchouli-db-macos-x86_64` |
| macOS | Apple Silicon | `patchouli-db-macos-aarch64` |
| Windows | x86_64 | `patchouli-db-windows-x86_64.exe` |

Each asset has a sibling `.sha256` file. The installers download both and stop
without installing when verification fails. Linux assets use a static musl
runtime so they do not depend on the host distribution's glibc version.

## Install a release

macOS or Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.sh | sh
```

The default binary location is `~/.local/bin/patchouli-db`. Add that directory to
`PATH` if it is not already present.

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/memorax-agent/dsh-patchouli/main/scripts/install.ps1 | iex
```

The default binary location is
`%LOCALAPPDATA%\Patchouli\bin\patchouli-db.exe`. Add that directory to `PATH` if
needed.

Both installers initialize the backend home at `~/.patchouli`. They preserve
existing configuration and validate it instead of replacing it. Optional
environment variables are:

- `PATCHOULI_VERSION`: release tag such as `v0.1.0`; defaults to the latest
  release.
- `PATCHOULI_INSTALL_DIR`: binary destination directory.
- `PATCHOULI_HOME`: initialized backend data/configuration directory.

Rerun the same installer to upgrade the executable. Stop a running daemon first
on Windows because the operating system may lock the executable.

On Unix, a newly created backend home, data directory, and runtime directory use
mode `0700`; configuration and database files use mode `0600`. Installation
refuses an existing backend home or managed file that grants group or other
users access, and reports the path so its owner can correct the permissions.
An upgrade validates configuration with the downloaded binary before replacing
the installed executable.

## Install from source

Rust stable and a C toolchain are required:

```bash
cargo install --locked --git https://github.com/memorax-agent/dsh-patchouli \
  --package patchouli-server
patchouli-db init --root "$HOME/.patchouli"
```

From a local checkout of the `main` branch, replace the first command with
`cargo install --locked --path crates/server` from the repository root.

## Initialized layout

`patchouli-db init --root <path>` creates and validates:

```text
<path>/
├── config.json              # backend policy
├── providers.json           # local/remote provider routing
├── patchouli.schema.json    # editor/validation schema for config.json
├── providers.schema.json    # editor/validation schema for providers.json
├── data/                    # default SQLite database location
│   └── artifacts/           # backend-managed content-addressed files
└── run/                     # Unix socket location
```

It creates missing files only. If an existing file is invalid, `init` reports
the error and leaves it unchanged.

Start the local backend in the foreground:

```bash
patchouli-db serve \
  --endpoint "$HOME/.patchouli/run/patchouli.sock" \
  --artifacts "$HOME/.patchouli/data/artifacts" \
  --providers "$HOME/.patchouli/providers.json" \
  --config "$HOME/.patchouli/config.json"
```

On Windows, use `\\.\pipe\patchouli` as the endpoint. Process supervision is a
separate deployment choice; `serve` works under launchd, systemd, a Windows
Service wrapper, or a parent plugin process.

## Uninstall

Stop the daemon, then remove only the installed executable. The installer never
deletes `PATCHOULI_HOME`; configuration and SQLite data remain available until
the user deliberately removes that directory.
