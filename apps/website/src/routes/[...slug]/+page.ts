import { error } from '@sveltejs/kit';
import { docs, getDoc } from '$lib/content';

export function entries() {
	return docs.map((doc) => ({ slug: doc.slug }));
}

export function load({ params }) {
	const slug = params.slug.replace(/\/+$/, '');
	if (!getDoc(slug)) throw error(404, 'Documentation page not found');
	return { slug };
}
