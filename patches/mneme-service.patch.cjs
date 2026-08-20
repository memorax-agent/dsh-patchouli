const BRIDGE_SOURCE = `
	  ctx.provide("goojfcMneme", ctx.patchouliGoojfc.createMnemeAdapter(service, summarizer, {
	      autoInject: cfg.autoInject,
	      maxInjectedItems: cfg.maxInjectedItems,
	      importanceThreshold: cfg.importanceThreshold,
	      session: (sessionId) => ctx.agents.get(sessionId)?.session,
	      getProfile: () => settings.getProfile(),
	      getRules: () => settings.getRules(),
	    }));`

/** @type {import('dsh-harmony').HarmonyPatch} */
module.exports = {
  id: 'goojfc-mneme-service',
  target: {
    package: '@modusensus/dsh-mneme',
    version: '0.3.7',
    file: 'lib/index.js',
  },
  select: 'VariableDeclaration[name.name="summarizer"][initializer.expression.name="createSummarizer"]',
  expect: 1,
  apply({ node, edit }) {
    edit.appendRight(node.parent.parent.getEnd(), BRIDGE_SOURCE)
  },
}
