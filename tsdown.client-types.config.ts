import { defineConfig } from 'tsdown'

export default defineConfig({
  entry: { index: 'src/client/index.tsx' },
  tsconfig: 'tsconfig.client.json',
  format: 'esm',
  platform: 'browser',
  target: 'es2023',
  deps: {
    dts: { alwaysBundle: [/^@memorax-agent\//] },
  },
  outDir: 'lib/client',
  clean: true,
  fixedExtension: false,
  outExtensions: () => ({ dts: '.d.ts' }),
  hash: false,
  sourcemap: true,
  dts: { eager: true, emitDtsOnly: true },
})
