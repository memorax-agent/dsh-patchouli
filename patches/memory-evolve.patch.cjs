const target = {
  package: 'dsh-memory-evolve',
  version: '0.1.0',
  file: 'lib/index.js',
}

// Source snapshot: ce7f0faa0e0240f117c29795e9224c0d9ed18183.

/** @type {import('dsh-harmony').HarmonyPatch[]} */
module.exports = [{
  id: 'goojfc-memory-evolve-order',
  target,
  select: 'VariableDeclaration[name.name="inject"] ArrayLiteralExpression',
  expect: 1,
  apply({ node, edit }) {
    edit.appendLeft(node.getEnd() - 1, ', "patchouliGoojfc"')
  },
}, {
  id: 'goojfc-memory-evolve-service',
  target,
  select: 'VariableDeclaration[name.name="counter"]',
  expect: 1,
  apply({ node, edit, ts }) {
    const statement = node.parent.parent
    if (!ts.isVariableStatement(statement)) {
      throw new Error('memory-evolve counter declaration is no longer a variable statement')
    }
    edit.appendRight(
      statement.getEnd(),
      String.raw`
  ctx.provide("goojfcMemoryEvolve", ctx.patchouliGoojfc.createMemoryEvolveAdapter({
    snapshot: (agent) => config.injectMemory
      ? renderSnapshot(getRuntime(), store, agent, counter, ctx.sessionTitle)
      : null,
    query: (target, agent, options = {}) => {
      const protectedLog = (target === "project" || target === "daily")
        && options.limit === undefined
        && options.recent === undefined
        && options.since === undefined
        && options.until === undefined;
      const queryOptions = {
        filter: options.filter,
        since: options.since,
        until: options.until,
        limit: protectedLog ? 50 : options.limit,
        recent: protectedLog ? true : options.recent,
      };
      let entries = store.query(target, agent, queryOptions);
      if (target === "key") {
        const branch = options.branch ?? gitBranch(agent.session.header.cwd);
        if (branch !== undefined && String(branch).trim()) {
          entries = entries.filter((entry) => {
            const scope = parseEntryBranches(entry);
            return scope === null || scope.includes(String(branch).trim());
          });
        }
      }
      return entries.map((entry) => stripEntryId(entry));
    },
  }));`,
    )
  },
}, {
  id: 'goojfc-memory-evolve-disable-native-snapshot',
  target,
  select: 'IfStatement:has(PropertyAccessExpression[expression.text="config"][name.text="injectMemory"])',
  expect: 1,
  apply({ node, edit, ts }) {
    let statement = node
    while (statement && !ts.isIfStatement(statement)) statement = statement.parent
    if (!statement) throw new Error('memory-evolve snapshot selector did not resolve to an if statement')
    edit.remove(statement.getFullStart(), statement.getEnd())
  },
}]
