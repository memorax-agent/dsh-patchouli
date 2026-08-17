const target = {
  package: '@openviking/dsh-memory-plugin',
  version: '0.1.0',
  files: ['index.mjs'],
}

const automaticEvents = new Set([
  'agent/session-start',
  'agent/pre-step',
  'session/event',
  'session/flush',
])

const bridgeSource = String.raw`
  ctx.provide("goojfcOpenViking", ctx.patchouliGoojfc.createOpenVikingAdapter(
      runtime,
      client,
  ));
`

/** @type {import('dsh-harmony').HarmonyPatch[]} */
module.exports = [{
  id: 'openviking-require-goojfc',
  target,
  select: 'VariableDeclaration[name.name="inject"] ArrayLiteralExpression',
  expect: 1,
  apply({ node, sourceFile, edit }) {
    edit.overwrite(
      node.getStart(sourceFile),
      node.getEnd(),
      '["agents", "sessions", "tools", "patchouliGoojfc"]',
    )
  },
}, {
  id: 'openviking-expose-native-service',
  target,
  select: 'VariableDeclaration[name.name="runtime"]',
  expect: 1,
  apply({ node, sourceFile, edit, ts }) {
    const statement = node.parent.parent
    if (!ts.isVariableStatement(statement)) {
      throw new Error('OpenViking runtime declaration is no longer a variable statement')
    }
    edit.appendRight(statement.getEnd(sourceFile), bridgeSource)
  },
}, {
  id: 'openviking-disable-native-automatic-hooks',
  target,
  select: 'CallExpression[expression.expression.text="ctx"][expression.name.text="on"]',
  expect: 5,
  apply({ node, sourceFile, edit, ts }) {
    const event = node.arguments[0]
    if (event !== undefined && ts.isStringLiteral(event) && automaticEvents.has(event.text)) {
      edit.overwrite(node.getStart(sourceFile), node.getEnd(), 'undefined')
    }
  },
}]
