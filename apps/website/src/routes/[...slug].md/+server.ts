import { error } from '@sveltejs/kit';
import { docs, getDoc } from '$lib/content';
import type { RequestHandler } from './$types';

export const prerender = true;

export function entries() {
	return docs.map((doc) => ({ slug: doc.slug }));
}

export const GET: RequestHandler = ({ params }) => {
	const doc = getDoc(params.slug);
	if (!doc) throw error(404, 'Documentation page not found');
	return new Response(doc.markdown, { headers: { 'content-type': 'text/markdown; charset=utf-8' } });
};
