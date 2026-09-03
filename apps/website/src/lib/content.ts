import type { Component } from "svelte";
import type { DocHeading } from "./table-of-contents";

/** A documentation navigation group. */
export type DocSection = "Get started" | "Guides" | "Reference";

/** Front matter generated for a documentation page. */
export interface DocMetadata {
  title: string;
  description: string;
  toc?: DocHeading[];
}

/** One Markdown documentation page and its stable URL slug. */
export interface Doc extends DocMetadata {
  slug: string;
  section: DocSection;
  component: Component;
  markdown: string;
  toc: DocHeading[];
}

type MarkdownModule = { default: Component; metadata: DocMetadata };
const modules = import.meta.glob<MarkdownModule>("/src/content/docs/**/*.md", { eager: true });
const sources = import.meta.glob<string>("/src/content/docs/**/*.md", {
  eager: true,
  query: "?raw",
  import: "default",
});

const navigationOrder = [
  "overview",
  "quick-start",
  "guides/local-development",
  "guides/ideas",
  "guides/epics",
  "guides/milestones",
  "guides/tasks",
  "guides/lifecycle",
  "guides/inspect-work",
  "guides/releases",
  "guides/snapshots",
  "guides/automation",
  "reference/manual",
  "reference/deps",
] as const;

/** Documentation sections in display order. */
export const docSections: DocSection[] = ["Get started", "Guides", "Reference"];

function sectionFor(slug: string): DocSection {
  if (slug.startsWith("guides/")) return "Guides";
  if (slug.startsWith("reference/")) return "Reference";
  return "Get started";
}

function sourceToSlug(source: string): string {
  return source.replace("/src/content/docs/", "").replace(/\.md$/, "");
}

export const docs: Doc[] = Object.entries(modules)
  .map(([source, module]) => {
    const slug = sourceToSlug(source);
    return {
      ...module.metadata,
      slug,
      section: sectionFor(slug),
      component: module.default,
      markdown: sources[source],
      toc: module.metadata.toc ?? [],
    };
  })
  .sort((left, right) => {
    const leftIndex = navigationOrder.indexOf(left.slug as (typeof navigationOrder)[number]);
    const rightIndex = navigationOrder.indexOf(right.slug as (typeof navigationOrder)[number]);
    return (leftIndex < 0 ? Number.MAX_SAFE_INTEGER : leftIndex) -
      (rightIndex < 0 ? Number.MAX_SAFE_INTEGER : rightIndex);
  });

/** Find a documentation page by its route slug. */
export function getDoc(slug: string): Doc | undefined {
  return docs.find((doc) => doc.slug === slug);
}

/** Find the pages immediately before and after a documentation page. */
export function getAdjacentDocs(slug: string): { previous?: Doc; next?: Doc } {
  const index = docs.findIndex((doc) => doc.slug === slug);
  return {
    previous: index > 0 ? docs[index - 1] : undefined,
    next: index >= 0 && index < docs.length - 1 ? docs[index + 1] : undefined,
  };
}
