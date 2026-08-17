const indexTarget = {
  package: '@modusensus/dsh-mneme',
  version: '0.3.7',
  files: ['lib/index.js'],
}

const summarizeTarget = {
  package: '@modusensus/dsh-mneme',
  version: '0.3.7',
  files: ['lib/summarize.js'],
}

function removeStatement(node, edit, ts) {
  let statement = node
  while (statement && !ts.isStatement(statement)) statement = statement.parent
  if (!statement) throw new Error('Mneme passive selector did not resolve to a statement')
  edit.remove(statement.getFullStart(), statement.getEnd())
}

/** @type {import('dsh-harmony').HarmonyPatch[]} */
module.exports = [{
  id: 'goojfc-mneme-disable-native-injector',
  target: indexTarget,
  select: 'CallExpression[expression.expression.text="ctx"][expression.name.text="inject"]:has(CallExpression[expression.text="createInjector"])',
  expect: 1,
  apply({ node, edit, ts }) {
    removeStatement(node, edit, ts)
  },
}, {
  id: 'goojfc-mneme-disable-native-summarize-hook',
  target: summarizeTarget,
  select: 'VariableDeclaration[name.name="unsubscribe"][initializer.expression.expression.name="ctx"][initializer.expression.name.name="on"]',
  expect: 1,
  apply({ node, edit, ts }) {
    removeStatement(node, edit, ts)
  },
}, {
  id: 'goojfc-mneme-remove-native-unsubscribe',
  target: summarizeTarget,
  select: 'ExpressionStatement:has(Identifier[name="unsubscribe"])',
  expect: 1,
  apply({ node, edit, ts }) {
    removeStatement(node, edit, ts)
  },
}, {
  id: 'goojfc-mneme-expose-summarize',
  target: summarizeTarget,
  select: 'ObjectLiteralExpression:has(MethodDeclaration[name.name="dispose"])',
  expect: 1,
  apply({ node, edit }) {
    edit.appendLeft(node.getStart() + 1, '\n    summarize,')
  },
}]
