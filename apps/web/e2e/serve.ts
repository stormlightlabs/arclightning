import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

/** The `arcl` binary the tests run: `ARCL_BIN`, or the debug build in Cargo's target directory. */
export const ARCL =
  process.env.ARCL_BIN ??
  join(process.env.CARGO_TARGET_DIR ?? fileURLToPath(new URL("../../../target", import.meta.url)), "debug", "arcl");

/** `arcl serve` on a scratch project, with the CLI pointed at the same project. */
export interface Served {
  /** The URL `arcl serve` printed, with the token for the first page load. */
  url: string;
  /** Run `arcl <args> --json` in the project and return the envelope's `data`. */
  cli: (...args: string[]) => any;
  /** Stop the server and delete the project. */
  stop: () => Promise<void>;
}

/** How long one `arcl` command, or `arcl serve` printing its URL, may take, in ms. */
export const SETUP_TIMEOUT = 10_000;

/** How long a stopped server may take to exit before it is killed, in ms. */
const KILL_GRACE = 5_000;

/**
 * The cleanups of every scratch project this worker has started and not yet
 * stopped. Each is added as soon as its directory exists, so a setup that
 * hangs or throws partway still leaves something `stopAll` can clean.
 */
const live = new Set<() => Promise<void>>();

/**
 * Initialize a scratch project in a new temporary directory and start
 * `arcl serve` on an ephemeral port there. The binary embeds the web app it was
 * built with, so build `apps/web` before `cargo build -p arcl-cli`, as
 * `pnpm test:e2e` does. Setup fails after `SETUP_TIMEOUT` rather than hang.
 * Call `stopAll` in `afterEach`, which also cleans up a setup that never
 * returned.
 */
export async function serveScratch(): Promise<Served> {
  if (!existsSync(ARCL)) throw new Error(`${ARCL} does not exist: run \`cargo build -p arcl-cli\` or set ARCL_BIN`);
  const dir = mkdtempSync(join(tmpdir(), "arcl-e2e-"));
  let server: ChildProcess | undefined;
  let exited: Promise<void> = Promise.resolve();
  const stop = async () => {
    live.delete(stop);
    if (server && server.exitCode === null && server.signalCode === null) {
      const child = server;
      child.kill();
      const timer = setTimeout(() => child.kill("SIGKILL"), KILL_GRACE);
      await exited;
      clearTimeout(timer);
    }
    rmSync(dir, { recursive: true, force: true });
  };
  live.add(stop);
  const cli = (...args: string[]) =>
    JSON.parse(
      execFileSync(ARCL, [...args, "--json"], {
        cwd: dir,
        encoding: "utf8",
        timeout: SETUP_TIMEOUT,
        killSignal: "SIGKILL",
      }),
    ).data;
  // Clean up when setup fails partway, since the caller gets no `stop` then.
  try {
    cli("init");
    const child = spawn(ARCL, ["serve", "--json"], { cwd: dir, stdio: ["ignore", "pipe", "inherit"] });
    server = child;
    exited = new Promise<void>((done) => child.once("exit", () => done()));
    let timer: ReturnType<typeof setTimeout> | undefined;
    const line = await new Promise<string>((done, fail) => {
      timer = setTimeout(() => fail(new Error(`arcl serve printed no URL within ${SETUP_TIMEOUT} ms`)), SETUP_TIMEOUT);
      child.once("error", fail);
      child.once("exit", (code: number | null) =>
        fail(new Error(`arcl serve exited with ${code} before printing its URL`)),
      );
      createInterface({ input: child.stdout! }).once("line", done);
    }).finally(() => clearTimeout(timer));
    return { url: JSON.parse(line).data.url, cli, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}

/** Stop every scratch project this worker started and has not stopped. */
export async function stopAll() {
  await Promise.all([...live].map((stop) => stop()));
}
