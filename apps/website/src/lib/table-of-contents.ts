type HeadingNode = {
  type?: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: HeadingNode[];
  value?: string;
};

type MetadataFile = { data: Record<string, unknown> };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function textContent(node: HeadingNode): string {
  if (typeof node.value === "string") return node.value;
  return (node.children ?? []).map(textContent).join("");
}

function collectHeadings(node: HeadingNode, headings: DocHeading[]): void {
  if (node.type === "element" && (node.tagName === "h2" || node.tagName === "h3")) {
    const slug = node.properties?.id;
    const title = textContent(node).trim();
    if (typeof slug === "string" && title) {
      headings.push({ title, slug, level: node.tagName === "h2" ? 2 : 3 });
    }
  }
  for (const child of node.children ?? []) collectHeadings(child, headings);
}

/** Add rendered h2 and h3 headings to mdsvex front matter. */
export function extractTableOfContents() {
  return (tree: HeadingNode, file: MetadataFile): void => {
    if (!isRecord(file.data.fm)) return;
    const toc: DocHeading[] = [];
    collectHeadings(tree, toc);
    file.data.fm = { ...file.data.fm, toc };
  };
}

export interface DocHeading {
  title: string;
  slug: string;
  level: 2 | 3;
}
