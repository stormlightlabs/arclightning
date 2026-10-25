import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { BUILDS, STAMP, upToDate } from "./e2e-builds.mjs";

describe("upToDate", () => {
  let web = "";

  /** Write `contents` to `path` under the fake `apps/web`. */
  const write = (path, contents = "") => {
    mkdirSync(dirname(join(web, path)), { recursive: true });
    writeFileSync(join(web, path), contents);
  };

  /** Leave `build` as a successful run would, stamped with `hash`. */
  const built = (build, hash) => {
    for (const path of build.outputs) write(path.endsWith("client") ? `${path}/_app/app.js` : path);
    write(`${build.out}/${STAMP}`, hash);
  };

  beforeEach(() => {
    web = mkdtempSync(join(tmpdir(), "e2e-builds-"));
  });
  afterEach(() => rmSync(web, { recursive: true, force: true }));

  it("is true when the stamp matches and every output is there", () => {
    built(BUILDS.web, "abc");
    built(BUILDS.artifact, "def");
    expect(upToDate(web, BUILDS.web, "abc")).toBe(true);
    expect(upToDate(web, BUILDS.artifact, "def")).toBe(true);
  });

  it("is false without a stamp, or with another hash", () => {
    built(BUILDS.web, "abc");
    expect(upToDate(web, BUILDS.web, "xyz")).toBe(false);
    rmSync(join(web, BUILDS.web.out, STAMP));
    expect(upToDate(web, BUILDS.web, "abc")).toBe(false);
  });

  // A failed `vite build` deletes `.svelte-kit/output`, which `vite preview`
  // serves, and leaves `build/` as it was.
  it("is false once an output is gone, the stamp matching or not", () => {
    for (const name of ["web", "artifact"]) {
      for (const output of BUILDS[name].outputs) {
        built(BUILDS[name], "abc");
        rmSync(join(web, output), { recursive: true });
        expect(upToDate(web, BUILDS[name], "abc"), `${name} without ${output}`).toBe(false);
      }
    }
  });

  it("is false after `.svelte-kit` is deleted", () => {
    built(BUILDS.web, "abc");
    built(BUILDS.artifact, "abc");
    rmSync(join(web, ".svelte-kit"), { recursive: true });
    expect(upToDate(web, BUILDS.web, "abc")).toBe(false);
    expect(upToDate(web, BUILDS.artifact, "abc")).toBe(false);
  });
});
