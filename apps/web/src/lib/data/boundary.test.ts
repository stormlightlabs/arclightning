import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = join(import.meta.dirname, "..", "..");
const DATA = join(SRC, "lib", "data");
/**
 * Any mention of `fetch`, called or not, any `/api/` path literal, and any
 * use of the artifact host or a database reference.
 */
const REACHES_API = /\bfetch\b|["'`]\/api\/|\bclaude\??\.use\b|\.(?:doc|collection)\(/;

function sources(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return path === DATA ? [] : sources(path);
    return /\.(svelte|ts)$/.test(entry.name) && !entry.name.endsWith(".test.ts") ? [path] : [];
  });
}

describe("the data module", () => {
  it("is the only code that reaches the API", () => {
    const offenders = sources(SRC)
      .filter((path) => REACHES_API.test(readFileSync(path, "utf8")))
      .map((path) => relative(SRC, path));
    expect(offenders).toEqual([]);
  });

  it("catches fetch and the artifact's database however they are reached", () => {
    for (const source of [
      "fetch(url)",
      "const f = fetch;",
      "globalThis.fetch",
      "window['fetch']",
      "get(`/api/records`)",
      'window.claude?.use("db")',
      'claude.use("user")',
      'db.doc("views/work")',
      'db.collection("queue")',
    ]) {
      expect(REACHES_API.test(source), source).toBe(true);
    }
    expect(REACHES_API.test("prefetch()")).toBe(false);
  });
});
