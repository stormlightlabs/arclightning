import { highlightTree, tagHighlighter, tags } from "@lezer/highlight";
import { describe, expect, it } from "vitest";
import { recordFile } from "./record-file";

const highlighter = tagHighlighter([
  { tag: tags.processingInstruction, class: "mark" },
  { tag: tags.propertyName, class: "key" },
  { tag: tags.string, class: "string" },
  { tag: tags.heading, class: "heading" },
]);

/** The text of each highlighted span in `text`, by class. */
function highlighted(text: string): Record<string, string[]> {
  const tree = recordFile().language.parser.parse(text);
  const spans: Record<string, string[]> = {};
  highlightTree(tree, highlighter, (from, to, classes) => {
    for (const name of classes.split(" ")) (spans[name] ??= []).push(text.slice(from, to));
  });
  return spans;
}

describe("recordFile", () => {
  it("highlights the front matter as TOML and the body as Markdown", () => {
    const spans = highlighted('+++\nid = "arcl-t-1"\ntitle = "# Not a heading"\n+++\n\n# Body\n');
    // The body's heading mark is a processing instruction too.
    expect(spans.mark).toEqual(["+++", "+++", "#"]);
    expect(spans.key).toEqual(["id", "title"]);
    expect(spans.string).toEqual(['"arcl-t-1"', '"# Not a heading"']);
    expect(spans.heading?.join("")).toContain("Body");
    expect(spans.heading?.join("")).not.toContain("Not a heading");
  });

  it("reads TOML to the end while the closing line is missing", () => {
    const spans = highlighted('+++\ntitle = "Open"\n');
    expect(spans.key).toEqual(["title"]);
  });

  it("leaves a document without front matter as Markdown", () => {
    const spans = highlighted('Text\n+++\ntitle = "x"\n');
    expect(spans.key).toBeUndefined();
    expect(spans.mark).toBeUndefined();
  });
});
