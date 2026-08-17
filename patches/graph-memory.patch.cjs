/** @type {import('dsh-harmony').HarmonyLoaderPatch} */
module.exports = {
  id: 'goojfc-graph-memory-typescript',
  target: {
    package: 'graph-memory',
    version: '1.5.8',
    files: ['index.ts'],
  },
  loader: 'typescript',
}
