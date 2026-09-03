/** Public Arc Lightning website metadata. */
export const site = {
	name: 'Arc Lightning',
	title: 'Arc Lightning · Connected planning for developers and agents',
	description: 'A local-first project planning and execution system for developers and software agents.',
	url: 'https://arclightning.stormlightlabs.org',
	githubUrl: 'https://github.com/stormlightlabs/arclightning'
} as const;

/** Resolve a site pathname to its canonical public URL. */
export function absoluteUrl(pathname: string): string {
	return new URL(pathname, site.url).toString();
}
