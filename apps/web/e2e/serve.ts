import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

/** The `arcl` binary the tests run: `ARCL_BIN`, or the debug build in Cargo's target directory. */
export const ARCL = process.env.ARCL_BIN ?? join(process.env.CARGO_TARGET_DIR ?? fileURLToPath(new URL("../../../target", import.meta.url)), "debug", "arcl");

/** `arcl serve` on a scratch project, with the CLI pointed at the same project. */
export interface Served {
  /** The URL `arcl serve` printed, with the token for the first page load. */
  url: string;
  /** Run `arcl <args> --json` in the project and return the envelope's `data`. */
  cli: (...args: string[]) => any;
  /** Stop the server and delete the project. */
  stop: () => Promise<void>;
}

/**
 * Initialize a scratch project in a new temporary directory and start
 * `arcl serve` on an ephemeral port there. The binary embeds the web app it was
 * built with, so build `apps/web` before `cargo build -p arcl-cli`, as
 * `pnpm test:e2e` does.
 */
export async function serveScratch(): Promise<Served> {
  if (!existsSync(ARCL)) throw new Error(`${ARCL} does not exist: run \`cargo build -p arcl-cli\` or set ARCL_BIN`);
  const dir = mkdtempSync(join(tmpdir(), "arcl-e2e-"));
  const cli = (...args: string[]) => JSON.parse(execFileSync(ARCL, [...args, "--json"], { cwd: dir, encoding: "utf8" })).data;
  cli("init");
  const server: ChildProcess = spawn(ARCL, ["serve", "--json"], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
  const exited = new Promise<void>((done) => server.once("exit", () => done()));
  const line = await new Promise<string>((done, fail) => {
    server.once("exit", (code: number | null) => fail(new Error(`arcl serve exited with ${code} before printing its URL`)));
    createInterface({ input: server.stdout! }).once("line", done);
  });
  return {
    url: JSON.parse(line).data.url,
    cli,
    stop: async () => {
      server.kill();
      await exited;
      rmSync(dir, { recursive: true, force: true });
    },
  };
}
