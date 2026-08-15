# Contributing

1. Create a focused branch from `main`.
2. Keep each change within one clear capability boundary.
3. Run the same checks enforced by CI before opening a Pull Request:

   ```bash
   pnpm check
   cargo fmt --all -- --check
   cargo clippy --locked --workspace --all-targets -- -D warnings
   cargo test --locked --workspace
   bash -n scripts/install.sh
   ```

   On Windows, CI also parses `scripts/install.ps1` with the PowerShell parser.
4. Update architecture or development documentation when a public contract or workflow changes.
5. Do not commit secrets, local knowledge data, generated `lib/` output, or package archives.

Pull Requests should describe the user-visible behavior, the checks performed, and any new configuration.
