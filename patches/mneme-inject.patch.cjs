/** @type {import('dsh-harmony').HarmonyPatch} */
module.exports = {
  id: 'goojfc-mneme-inject',
  target: {
    package: '@modusensus/dsh-mneme',
    version: '0.3.7',
    file: 'lib/index.js',
  },
  select: 'VariableDeclaration[name.name="inject"] ArrayLiteralExpression',
  expect: 1,
  apply({ node, edit }) {
    edit.appendLeft(node.getEnd() - 1, ', "patchouliGoojfc", "agents"')
  },
}
