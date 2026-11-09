import { error } from '@sveltejs/kit';
import { docs, getDoc, getRedirect, redirects } from '$lib/content';
import type { RequestHandler } from './$types';

export const prerender = true;

/** Every page's slug, plus each retired slug, which serves its replacement. */
export function entries() {
	return [...docs.map((doc) => doc.slug), ...Object.keys(redirects)].map((slug) => ({ slug }));
}

export const GET: RequestHandler = ({ params }) => {
	const doc = getDoc(params.slug) ?? getDoc(getRedirect(params.slug) ?? '');
	if (!doc) throw error(404, 'Documentation page not found');
	return new Response(doc.markdown, { headers: { 'content-type': 'text/markdown; charset=utf-8' } });
};
