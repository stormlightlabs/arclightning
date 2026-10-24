import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = join(import.meta.dirname, "..", "..");
const DATA = join(SRC, "lib", "data");
/** Any mention of `fetch`, called or not, and any `/api/` path literal. */
const REACHES_API = /\bfetch\b|["'`]\/api\//;

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

  it("catches fetch however it is reached", () => {
    for (const source of ["fetch(url)", "const f = fetch;", "globalThis.fetch", "window['fetch']", "get(`/api/records`)"]) {
      expect(REACHES_API.test(source), source).toBe(true);
    }
    expect(REACHES_API.test("prefetch()")).toBe(false);
  });
});
