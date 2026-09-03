import type { Component } from "svelte";

/** Front matter retained from the existing documentation. */
export interface DocMetadata { title: string; description: string; }
/** One Markdown documentation page and its stable URL slug. */
export interface Doc extends DocMetadata { slug: string; component: Component; }

type MarkdownModule = { default: Component; metadata: DocMetadata };
const modules = import.meta.glob<MarkdownModule>("/src/content/docs/**/*.md", { eager: true });

export const docs: Doc[] = Object.entries(modules)
  .map(([path, module]) => ({
    ...module.metadata,
    slug: path.replace("/src/content/docs/", "").replace(/\.md$/, ""),
    component: module.default,
  }))
  .sort((left, right) => left.slug.localeCompare(right.slug));

/** Find a documentation page by its route slug. */
export function getDoc(slug: string): Doc | undefined {
  return docs.find((doc) => doc.slug === slug);
}
