import { error, redirect } from '@sveltejs/kit';
import { docs, getDoc, getRedirect, redirects } from '$lib/content';

export function entries() {
	return [...docs.map((doc) => doc.slug), ...Object.keys(redirects)].map((slug) => ({ slug }));
}

export function load({ params }) {
	const slug = params.slug.replace(/\/+$/, '');
	const target = getRedirect(slug);
	if (target) throw redirect(308, `/${target}/`);
	if (!getDoc(slug)) throw error(404, 'Documentation page not found');
	return { slug };
}
