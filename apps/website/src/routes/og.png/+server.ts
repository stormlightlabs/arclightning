import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

import { Resvg } from '@resvg/resvg-js';
import Opengraph from '$lib/components/Opengraph.svelte';
import satori from 'satori';
import { html } from 'satori-html';
import { render } from 'svelte/server';

export const prerender = true;

const width = 1200;
const height = 630;

function staticPath(file: string): string {
	return join(process.cwd(), 'static', file);
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
	return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

function stripSvelteComments(value: string): string {
	return value.replace(/<!--[\s\S]*?-->/g, '');
}

/** Render the social card to a 1200x630 PNG at build time. */
export async function GET(): Promise<Response> {
	const [googleSans, googleSansBold, ibmPlexSans, favicon] = await Promise.all([
		readFile(staticPath('og/fonts/google-sans-latin-normal.ttf')).then(toArrayBuffer),
		readFile(staticPath('og/fonts/google-sans-latin-700-normal.ttf')).then(toArrayBuffer),
		readFile(staticPath('og/fonts/ibm-plex-sans-latin-normal.ttf')).then(toArrayBuffer),
		readFile(staticPath('favicon.svg')).then((value) => `data:image/svg+xml;base64,${value.toString('base64')}`)
	]);
	const { body } = render(Opengraph, { props: { width, height, faviconDataUrl: favicon } });
	// satori types its input as a React node; satori-html builds the same
	// { type, props } tree without React, so the cast only bridges the types.
	const markup = html(stripSvelteComments(body)) as Parameters<typeof satori>[0];
	const svg = await satori(markup, {
		width,
		height,
		fonts: [
			{ name: 'Google Sans', data: googleSans, weight: 400, style: 'normal' },
			{ name: 'Google Sans', data: googleSansBold, weight: 700, style: 'normal' },
			{ name: 'IBM Plex Sans', data: ibmPlexSans, weight: 400, style: 'normal' }
		]
	});
	const png = new Resvg(svg, { fitTo: { mode: 'width', value: width } }).render().asPng();

	return new Response(toArrayBuffer(png), {
		headers: { 'cache-control': 'public, immutable, max-age=31536000', 'content-type': 'image/png' }
	});
}
