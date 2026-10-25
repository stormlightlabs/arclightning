/**
 * The web builds `e2e-build.mjs` makes, and whether one is up to date. Kept
 * apart from that script so a test can import it without building.
 */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** The file in a build's output directory that holds the hash of its inputs. */
export const STAMP = ".inputs";

/**
 * The files and directories, relative to the repository, that the web builds
 * read. A path that does not exist hashes as nothing. `pnpm-lock.yaml` names
 * the dependencies wanted and `node_modules/.pnpm/lock.yaml` the ones
 * installed, so a build made before `pnpm install` runs again after it.
 */
export const INPUTS = [
  "apps/web/src",
  "apps/web/static",
  "apps/web/package.json",
  "apps/web/svelte.config.js",
  "apps/web/tsconfig.json",
  "apps/web/vite.config.ts",
  "packages/ui/src",
  "packages/ui/package.json",
  "packages/ui/svelte.config.js",
  "packages/ui/tsconfig.json",
  "packages/ui/uno.config.mjs",
  "pnpm-workspace.yaml",
  "pnpm-lock.yaml",
  "node_modules/.pnpm/lock.yaml",
];

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
