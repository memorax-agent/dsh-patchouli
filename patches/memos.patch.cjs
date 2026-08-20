const target = {
  package: '@memtensor/memos-local-plugin',
  version: '2.0.16-beta.1',
  file: 'dist/adapters/deepseek-harness/index.js',
}

function removeStatement({ node, edit, ts }) {
  let statement = node
  while (statement && !ts.isExpressionStatement(statement)) statement = statement.parent
  if (!statement) throw new Error('MemOS hook selector did not resolve to an expression statement')
  edit.remove(statement.getFullStart(), statement.getEnd())
}

/** @type {import('dsh-harmony').HarmonyPatch[]} */
module.exports = [
  {
    id: 'memos-require-goojfc',
    target,
    select: 'VariableDeclaration[name.text="inject"] > ArrayLiteralExpression',
    expect: 1,
    apply({ node, sourceFile, edit }) {
      edit.appendLeft(node.getEnd() - 1, ', "patchouliGoojfc"')
    },
  },
  {
    id: 'memos-disable-native-recall',
    target,
    select: 'CallExpression[expression.expression.text="ctx"][expression.name.text="on"] > StringLiteral[text="agent/pre-step"]',
    expect: 1,
    apply: removeStatement,
  },
  {
    id: 'memos-disable-native-capture',
    target,
    select: 'CallExpression[expression.expression.text="ctx"][expression.name.text="on"] > StringLiteral[text="session/event"]',
    expect: 1,
    apply: removeStatement,
  },
  {
    id: 'memos-disable-native-dispose',
    target,
    select: 'CallExpression[expression.expression.text="ctx"][expression.name.text="on"] > StringLiteral[text="session/disposed"]',
    expect: 1,
    apply: removeStatement,
  },
  {
    id: 'memos-disable-native-guidance',
    target,
    select: 'CallExpression[expression.name.text="section"][expression.expression.name.text="systemPrompt"]',
    expect: 1,
    apply: removeStatement,
  },
  {
    id: 'memos-disable-native-tools',
    target,
    select: 'CallExpression[expression.text="registerDeepSeekHarnessTools"]',
    expect: 1,
    apply: removeStatement,
  },
  {
    id: 'memos-provide-bridge',
    target,
    select: 'VariableDeclaration[name.text="registrations"]',
    expect: 1,
    apply({ node, edit, ts }) {
      const statement = node.parent.parent
      if (!ts.isVariableStatement(statement)) {
        throw new Error('MemOS registrations declaration is no longer a variable statement')
      }
      edit.appendRight(statement.getEnd(), `
        let goojfcMemosDelegate;
        const goojfcMemos = {
            id: "memos",
            update(request, context) {
                if (!goojfcMemosDelegate) throw new Error("MemOS adapter is not ready");
                return goojfcMemosDelegate.update(request, context);
            },
            retrieve(request, context) {
                if (!goojfcMemosDelegate) throw new Error("MemOS adapter is not ready");
                return goojfcMemosDelegate.retrieve(request, context);
            },
        };
        registrations.push(ctx.provide("goojfcMemos", goojfcMemos));`)
    },
  },
  {
    id: 'memos-bind-bridge',
    target,
    select: 'CallExpression[expression.text="createDeepSeekHarnessBridge"]',
    expect: 1,
    apply({ node, edit, ts }) {
      let statement = node
      while (statement && !ts.isExpressionStatement(statement)) statement = statement.parent
      if (!statement) throw new Error('MemOS bridge selector did not resolve to an expression statement')
      edit.appendRight(statement.getEnd(), `
        goojfcMemosDelegate = ctx.patchouliGoojfc.createMemosAdapter(core, {
            profileId: config.profileId,
            searchTimeoutMs: foregroundSearchTimeoutMs,
            recallEnabled: config.recallEnabled,
            bridge,
            runWithLlmRoute: (route, operation) => routes.run(route, operation),
        });`)
    },
  },
]
