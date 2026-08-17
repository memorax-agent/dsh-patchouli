const BRIDGE_SOURCE = `
	ctx.provide("goojfcMnemon", ctx.patchouliGoojfc.createMnemonAdapter(runtime, lifecycle, {
				session(sessionId) {
					const agent = ctx.agents.get(sessionId);
					if (agent === undefined) return undefined;
					return {
						root: ctx.agents.roots().includes(agent),
						hotMemory: runtime.forAgent(agent).runtimeMemory.contextText(),
					};
				},
			}));`

/** @type {import('dsh-harmony').HarmonyPatch} */
module.exports = {
  id: 'goojfc-mnemon-service',
  target: {
    package: 'dsh-mnemon',
    version: '0.1.6',
    files: ['lib/index.js'],
  },
  select: 'VariableStatement:has(VariableDeclaration[name.name="lifecycle"] NewExpression[expression.name="MnemonLifecycle"])',
  expect: 1,
  apply({ node, edit }) {
    edit.appendRight(node.getEnd(), BRIDGE_SOURCE)
  },
}
