/** @type {import('dsh-harmony').HarmonyPatch} */
module.exports = {
  id: 'goojfc-mnemon-inject',
  target: {
    package: 'dsh-mnemon',
    version: '0.1.6',
    file: 'lib/index.js',
  },
  select: 'VariableDeclaration[name.name="inject"] ArrayLiteralExpression',
  expect: 1,
  apply({ node, edit }) {
    edit.appendLeft(node.getEnd() - 1, ', "patchouliGoojfc"')
  },
}
