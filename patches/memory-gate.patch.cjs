const indexTarget = {
  package: 'dsh-memory-gate',
  version: '0.9.0',
  files: ['lib/index.js'],
}

/** @type {import('dsh-harmony').HarmonyPatch[]} */
module.exports = [{
  id: 'goojfc-memory-gate-import',
  target: indexTarget,
  select: 'SourceFile',
  expect: 1,
  apply({ edit }) {
    edit.prepend('import { registerMemoryCommand } from "./commands.js";\nimport { sessionScopeKey, workspaceScopeKey } from "./scope.js";\n')
  },
}, {
  id: 'goojfc-memory-gate-order',
  target: indexTarget,
  select: 'VariableDeclaration[name.name="inject"] ArrayLiteralExpression',
  expect: 1,
  apply({ node, edit }) {
    edit.appendLeft(node.getEnd() - 1, ', "patchouliGoojfc"')
  },
}, {
  id: 'goojfc-memory-gate-service',
  target: indexTarget,
  select: 'VariableDeclaration[name.name="service"]',
  expect: 1,
  apply({ node, edit, ts }) {
    const statement = node.parent.parent
    if (!ts.isVariableStatement(statement)) {
      throw new Error('memory-gate service declaration is no longer a variable statement')
    }
    edit.appendRight(
      statement.getEnd(),
      '\n    ctx.provide("goojfcMemoryGate", ctx.patchouliGoojfc.createMemoryGateAdapter(service, { sessionScopeKey, workspaceScopeKey, recordInjection(recall, context) { service.repository.recordInjection(recall.runId, context.sessionId, service.mode, [...recall.claimIds], context.injectionId); } }));',
    )
  },
}, {
  id: 'goojfc-memory-gate-disable-native-harness',
  target: indexTarget,
  select: 'CallExpression[expression.name="attachHarness"]',
  expect: 1,
  apply({ node, sourceFile, edit }) {
    edit.overwrite(
      node.getStart(sourceFile),
      node.getEnd(),
      'ctx.effect(() => registerMemoryCommand(ctx.commands, service), "memory-gate: command")',
    )
  },
}]
