/** @type {import('dsh-harmony').HarmonyPatch} */
module.exports = {
  id: 'goojfc-mnemon-passive-runtime',
  target: {
    package: 'dsh-mnemon',
    version: '0.1.6',
    files: ['lib/index.js'],
  },
  select: [
    'CallExpression[expression.expression.name="ctx"][expression.name.name="effect"][arguments.0.body.expression.expression.name="lifecycle"][arguments.0.body.expression.name.name="start"]',
    'CallExpression[expression.name="registerGuidance"]',
    'CallExpression[expression.name="registerRuntimeMemoryContext"]',
  ].join(', '),
  expect: 3,
  apply({ node, edit, ts }) {
    let statement = node
    while (statement && !ts.isExpressionStatement(statement)) statement = statement.parent
    if (!statement) throw new Error('Mnemon passive selector did not resolve to a statement')
    edit.remove(statement.getFullStart(), statement.getEnd())
  },
}
