const target = {
  package: '@furongjun1999/dsh-memory',
  version: '0.2.8',
  files: ['lib/index.js'],
}

/** @type {import('dsh-harmony').HarmonyPatch[]} */
module.exports = [{
  id: 'goojfc-lingshu-order',
  target,
  select: 'VariableDeclaration[name.name="inject"] ArrayLiteralExpression',
  expect: 1,
  apply({ node, edit }) {
    edit.appendLeft(node.getEnd() - 1, ', "patchouliGoojfc"')
  },
}, {
  id: 'goojfc-lingshu-service',
  target,
  select: 'VariableDeclaration[name.name="bridge"]',
  expect: 1,
  apply({ node, edit, ts }) {
    const statement = node.parent.parent
    if (!ts.isVariableStatement(statement)) {
      throw new Error('Lingshu bridge declaration is no longer a variable statement')
    }
    edit.appendRight(
      statement.getEnd(),
      '\n    ctx.provide("goojfcLingshu", ctx.patchouliGoojfc.createLingshuAdapter(bridge, config.memory));',
    )
  },
}, {
  id: 'goojfc-lingshu-disable-native-capture',
  target,
  select: 'CallExpression[expression.text="installMemoryHooks"]',
  expect: 1,
  apply({ node, edit, ts }) {
    let statement = node
    while (statement && !ts.isExpressionStatement(statement)) statement = statement.parent
    if (!statement) throw new Error('Lingshu hook selector did not resolve to a statement')
    edit.remove(statement.getFullStart(), statement.getEnd())
  },
}, {
  id: 'goojfc-lingshu-fiber-owned-dispose',
  target,
  select: 'CallExpression[expression.expression.text="ctx"][expression.name.text="effect"]',
  expect: 1,
  apply({ node, sourceFile, edit, ts }) {
    let statement = node
    while (statement && !ts.isExpressionStatement(statement)) statement = statement.parent
    if (!statement) throw new Error('Lingshu effect selector did not resolve to a statement')
    const callback = node.arguments[0]
    if (!callback || !ts.isArrowFunction(callback) || !ts.isBlock(callback.body)) {
      throw new Error('Lingshu effect no longer has an arrow-function body')
    }
    const returned = callback.body.statements.find(ts.isReturnStatement)?.expression
    if (!returned) throw new Error('Lingshu effect no longer returns a disposer')
    edit.overwrite(
      statement.getStart(sourceFile),
      statement.getEnd(),
      `return ${returned.getText(sourceFile)};`,
    )
  },
}]
