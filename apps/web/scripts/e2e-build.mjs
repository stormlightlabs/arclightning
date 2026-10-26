#!/usr/bin/env node
/**
 * Build what the Playwright tests load, and skip each web build whose inputs
 * have not changed since it last ran. Name the builds as arguments:
 *
 * - `web`: the `arcl serve` build in `build/`, as `pnpm build` makes it
 * - `artifact`: the claude.ai artifact build in `build-artifact/`, as
 *   `pnpm build:artifact` makes it
 * - `arcl`: `cargo build -p arcl-cli`, which embeds `build/index.html`, so it
 *   runs after `web`
 *
 * The web builds that are out of date run at the same time. A build that
 * succeeds writes the hash of its inputs to `.inputs` in its output
 * directory, and this script deletes that file before it builds there. A
 * build is up to date when that file holds the current hash and its outputs,
 * `vite preview`'s among them, are still there. Cargo tracks its own inputs,
 * so `arcl` always runs. Set `ARCL_REBUILD=1` to build regardless.
 */
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { BUILDS, INPUTS, STAMP, upToDate } from "./e2e-builds.mjs";

const WEB = fileURLToPath(new URL("..", import.meta.url));
const ROOT = join(WEB, "../..");

/** Tests, stories and what failed tests leave behind, which no build reads. */
const UNREAD = /\.test\.ts$|\.stories\.ts$|\/__tests__\/|\/__screenshots__\/|\/\.vitest-attachments\//;

/** Every input file under `path`, relative to the repository, in a fixed order. */
function files(path) {
  const full = join(ROOT, path);
  if (!existsSync(full)) return [];
  if (!statSync(full).isDirectory()) return UNREAD.test(`/${path}`) ? [] : [path];
  return readdirSync(full)
    .sort()
    .flatMap((name) => files(`${path}/${name}`));
}

/** The hash of the build's name and of every input file's path and contents. */
function inputHash(name) {
  const hash = createHash("sha256").update(name);
  for (const file of INPUTS.flatMap(files)) hash.update(`\0${file}\0`).update(readFileSync(join(ROOT, file)));
  return hash.digest("hex");
}

/** Run `command`, and resolve with its exit code and output, which shows once it ends. */
function run(command, args, options = {}) {
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd: WEB,
      env: { ...process.env, ...options.env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    child.stdout.on("data", (chunk) => (output += chunk));
    child.stderr.on("data", (chunk) => (output += chunk));
    child.on("close", (code) => {
      process.stdout.write(`$ ${command} ${args.join(" ")}\n${output}`);
      resolve(code ?? 1);
    });
  });
}

const names = process.argv.slice(2);
const unknown = names.filter((name) => name !== "arcl" && !(name in BUILDS));
if (names.length === 0 || unknown.length) {
  console.error(
    `usage: e2e-build.mjs web|artifact|arcl...${unknown.length ? ` (unknown: ${unknown.join(", ")})` : ""}`,
  );
  process.exit(2);
}

const stale = [];
for (const name of names.filter((name) => name in BUILDS)) {
  const hash = inputHash(name);
  if (!process.env.ARCL_REBUILD && upToDate(WEB, BUILDS[name], hash)) console.log(`${BUILDS[name].out}/ is up to date`);
  else stale.push({ name, stamp: join(WEB, BUILDS[name].out, STAMP), hash });
}

if (stale.length) {
  // A build that fails leaves no stamp, so the next run builds again.
  for (const { stamp } of stale) rmSync(stamp, { force: true });
  // Sync here, once, what both builds read: the ui package, and the app's
  // `.svelte-kit/tsconfig.json`, which `tsconfig.json` extends. The artifact
  // build syncs only `.svelte-kit/artifact/`, and two builds that each synced
  // would write the same files at once.
  for (const args of [
    ["--filter", "@arclightning/ui", "exec", "svelte-kit", "sync"],
    ["exec", "svelte-kit", "sync"],
  ]) {
    if ((await run("pnpm", args)) !== 0) process.exit(1);
  }
  const codes = await Promise.all(
    stale.map(({ name }) =>
      run("pnpm", ["exec", "vite", "build"], { env: { VITE_ARCL_TARGET: BUILDS[name].target ?? "" } }),
    ),
  );
  if (codes.some((code) => code !== 0)) process.exit(1);
  for (const { stamp, hash } of stale) writeFileSync(stamp, hash);
}

if (names.includes("arcl")) {
  const cargo = spawn("cargo", ["build", "-p", "arcl-cli"], { cwd: ROOT, stdio: "inherit" });
  const code = await new Promise((resolve) => cargo.on("close", resolve));
  if (code !== 0) process.exit(code ?? 1);
}
