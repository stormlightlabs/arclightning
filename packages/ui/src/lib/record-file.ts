import { markdown } from "@codemirror/lang-markdown";
import { StreamLanguage, type LanguageSupport } from "@codemirror/language";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { parseMixed } from "@lezer/common";
import { tags } from "@lezer/highlight";
import type { MarkdownConfig } from "@lezer/markdown";

/**
 * The line that opens and closes a record file's front matter. It is typed
 * as `string` so a check on one line does not narrow the next line's text.
 */
const DELIMITER: string = "+++";

const tomlLanguage = StreamLanguage.define(toml);

/**
 * A Markdown extension for a record file's front matter: the lines from a
 * `+++` line at the very start of the document to the next `+++` line,
 * parsed as TOML. A front matter block with no closing line runs to the end
 * of the document, as the editor shows it while the person types.
 */
export const frontMatter: MarkdownConfig = {
  defineNodes: [
    { name: "FrontMatter", block: true },
    { name: "FrontMatterMark", style: tags.processingInstruction },
    { name: "FrontMatterContent" },
  ],
  parseBlock: [
    {
      name: "FrontMatter",
      before: "HorizontalRule",
      parse(cx, line) {
        if (cx.lineStart !== 0 || line.text !== DELIMITER) return false;
        const start = cx.lineStart;
        const children = [cx.elt("FrontMatterMark", start, start + DELIMITER.length)];
        let contentStart = -1;
        let end = start + DELIMITER.length;
        let closed = false;
        while (cx.nextLine()) {
          if (contentStart < 0) contentStart = cx.lineStart;
          if (line.text === DELIMITER) {
            if (cx.lineStart > contentStart)
              children.push(cx.elt("FrontMatterContent", contentStart, cx.lineStart - 1));
            children.push(cx.elt("FrontMatterMark", cx.lineStart, cx.lineStart + DELIMITER.length));
            end = cx.lineStart + DELIMITER.length;
            closed = true;
            cx.nextLine();
            break;
          }
          end = cx.lineStart + line.text.length;
        }
        if (!closed && contentStart >= 0 && end > contentStart)
          children.push(cx.elt("FrontMatterContent", contentStart, end));
        cx.addElement(cx.elt("FrontMatter", start, end, children));
        return true;
      },
    },
  ],
  wrap: parseMixed((node) => (node.name === "FrontMatterContent" ? { parser: tomlLanguage.parser } : null)),
};

/** A record file: TOML front matter between `+++` lines, then a Markdown body. */
export function recordFile(): LanguageSupport {
  return markdown({ extensions: [frontMatter] });
}
