import type { Handle } from '@sveltejs/kit';
import { getRedirect, redirectPage } from '$lib/content';

/**
 * Serve each retired docs URL as a redirect page that keeps the URL fragment.
 * SvelteKit's prerendered `redirect()` page drops it. The prerenderer writes
 * an `index.html` only for a content type of exactly `text/html`; the page
 * declares its charset itself.
 */
export const handle: Handle = async ({ event, resolve }) => {
	const target = getRedirect(event.url.pathname.replace(/^\/+|\/+$/g, ''));
	if (target) {
		return new Response(redirectPage(target), { headers: { 'content-type': 'text/html' } });
	}
	return resolve(event);
};
