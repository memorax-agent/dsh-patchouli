const target = {
  package: '@vectorize-io/hindsight-coding-agents',
  version: '0.3.4',
  files: ['dist/dsh.js'],
}

const bridgeSource = String.raw`
  ctx.provide("goojfcHindsight", ctx.patchouliGoojfc.createHindsightAdapter({
      harness: HARNESS2,
      maxReflectTimeoutMs: HOOK_REFLECT_CAP_MS,
      workspaceFor,
      ensureSeeded,
      retainTranscript(workspace, sessionId, events) {
        return workspace.core.onTranscript(sessionId, readDshEvents(events));
      },
      readEvents: readDshEvents,
      retainStamp(workspace, sessionId) {
        return buildRetainStamp(workspace.core.cfg, {
          directory: workspace.root,
          harness: HARNESS2,
          bankId: workspace.core.bankId,
          sessionId,
        });
      },
      reflectQuery: buildReflectQuery,
      operationId: uuidV5,
    }));`

/** @type {import('dsh-harmony').HarmonyPatch[]} */
module.exports = [{
  id: 'goojfc-hindsight-order',
  target,
  select: 'VariableDeclaration[name.name="inject"] ArrayLiteralExpression',
  expect: 1,
  apply({ node, edit }) {
    edit.appendLeft(node.getEnd() - 1, ', "patchouliGoojfc"')
  },
}, {
  id: 'goojfc-hindsight-disable-native-hooks',
  target,
  select: 'CallExpression[expression.expression.text="ctx"][expression.name.text="on"]',
  expect: 4,
  apply({ node, sourceFile, edit }) {
    edit.overwrite(node.getStart(sourceFile), node.getEnd(), 'undefined')
  },
}, {
  id: 'goojfc-hindsight-service',
  target,
  select: 'VariableDeclaration[name.name="hooks"][initializer.expression.name="createDshHooks"]',
  expect: 1,
  apply({ node, edit, ts }) {
    const statement = node.parent.parent
    if (!ts.isVariableStatement(statement)) {
      throw new Error('Hindsight hooks declaration is no longer a variable statement')
    }
    edit.appendRight(statement.getEnd(), bridgeSource)
  },
}]
