/** Public Arc Lightning website metadata. */
export const site = {
	name: 'Arc Lightning',
	title: 'Arc Lightning · Connected planning for developers and agents',
	description: 'A local-first project planning and execution system for developers and software agents.',
	url: 'https://arcl.stormlightlabs.org',
	githubUrl: 'https://github.com/stormlightlabs/arclightning',
	/** The social card `/og.png` renders. */
	image: {
		url: 'https://arcl.stormlightlabs.org/og.png',
		alt: 'Arc Lightning: turn intent into ready work.',
		width: 1200,
		height: 630
	}
} as const;

/** Resolve a site pathname to its canonical public URL. */
export function absoluteUrl(pathname: string): string {
	return new URL(pathname, site.url).toString();
}
