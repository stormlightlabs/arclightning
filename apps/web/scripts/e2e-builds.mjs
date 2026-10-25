/**
 * The web builds `e2e-build.mjs` makes, and whether one is up to date. Kept
 * apart from that script so a test can import it without building.
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** The file in a build's output directory that holds the hash of its inputs. */
export const STAMP = ".inputs";

/**
 * Each web build: its output directory, the `VITE_ARCL_TARGET` it builds
 * with, and the paths, relative to `apps/web`, that must exist after it. The
 * `.svelte-kit` output is what `vite preview` serves, and the next `vite
 * build` in that `outDir` deletes it before it writes `build/`.
 */
export const BUILDS = {
  web: { out: "build", target: undefined, outputs: ["build/index.html", ".svelte-kit/output/client"] },
  artifact: {
    out: "build-artifact",
    target: "artifact",
    outputs: ["build-artifact/index.html", ".svelte-kit/artifact/output/client"],
  },
};

/**
 * Whether `build` under `web` is up to date with inputs that hash to `hash`:
 * its stamp holds that hash and every output it leaves is still there.
 *
 * @param {string} web the `apps/web` directory
 * @param {{ out: string, outputs: string[] }} build one of `BUILDS`
 * @param {string} hash the hash of the build's inputs
 */
export function upToDate(web, build, hash) {
  const stamp = join(web, build.out, STAMP);
  if (!existsSync(stamp) || readFileSync(stamp, "utf8") !== hash) return false;
  return build.outputs.every((path) => existsSync(join(web, path)));
}
