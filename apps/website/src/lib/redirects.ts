/**
 * Retired page slugs and the slugs of the pages that replace them. The build
 * prerenders each retired URL as a redirect. Each target is a current page, so
 * no redirect leads to another. The module imports nothing, so the e2e tests
 * read the same map the build uses.
 */
export const redirects: Readonly<Record<string, string>> = {
	overview: 'explanation/overview',
	'quick-start': 'tutorials/quick-start',
	'guides/agents': 'how-to/set-up-an-agent',
	'guides/api': 'how-to/query-graphql',
	'guides/artifact': 'how-to/publish-the-artifact',
	'guides/automation': 'how-to/script-arcl',
	'guides/captures': 'how-to/capture-and-triage',
	'guides/compaction': 'how-to/compact-old-work',
	'guides/epics': 'how-to/write-a-spec',
	'guides/ideas': 'how-to/capture-and-triage',
	'guides/inspect-work': 'how-to/find-work',
	'guides/lifecycle': 'explanation/readiness',
	'guides/local-development': 'how-to/develop-locally',
	'guides/mcp': 'how-to/connect-mcp',
	'guides/milestones': 'how-to/plan-from-a-file',
	'guides/notes': 'how-to/keep-notes',
	'guides/phases': 'how-to/plan-from-a-file',
	'guides/plans': 'how-to/plan-from-a-file',
	'guides/releases': 'how-to/cut-a-release',
	'guides/rest': 'reference/rest',
	'guides/specs': 'how-to/write-a-spec',
	'guides/tasks': 'how-to/work-a-task',
	'guides/web-app': 'how-to/use-the-web-app',
	'guides/workspace': 'how-to/share-the-workspace'
};
