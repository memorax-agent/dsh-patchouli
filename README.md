# Patchouli Documentation

This branch contains only the source and build configuration for the
[Patchouli documentation site](https://memorax-ai.github.io/dsh-patchouli/).
Application code is developed on the
[`main`](https://github.com/memorax-ai/dsh-patchouli/tree/main) branch.

## Development

```bash
corepack enable
pnpm install
pnpm docs:dev
```

Build the production site with `pnpm docs:build`.
