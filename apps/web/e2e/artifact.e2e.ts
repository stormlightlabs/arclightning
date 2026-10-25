import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import recorded from "../src/lib/data/artifact.fixture.json" with { type: "json" };

/**
 * The artifact build, opened at a path of the host's choosing with a stand-in
 * for claude.ai's `window.claude`, whose database holds what `arcl web
 * export` wrote for a scratch project.
 */
const PAGE = "https://artifact.test/embed/";
const HTML = readFileSync(new URL("../build-artifact/index.html", import.meta.url), "utf8");
const BLOCKED = "arcl-t-01M5RVP5WQQ1JH509T612ZZ60W";
const CAPTURE = "arcl-c-01M5RVP5XHT7D6S1NWEJ9BEEYG";

/** Install a `window.claude` whose `db` is an in-memory store seeded with `documents`, or none. */
function installHost(seed: { documents: Record<string, unknown> | null }) {
  type Body = Record<string, unknown>;
  const store = new Map<string, Body>(Object.entries(seed.documents ?? {}) as [string, Body][]);
  const listeners = new Set<() => void>();
  let minted = 0;
  const notify = () => setTimeout(() => listeners.forEach((listener) => listener()), 0);
  const listen = (deliver: () => void) => {
    listeners.add(deliver);
    setTimeout(deliver, 0);
    return () => listeners.delete(deliver);
  };
  const snapshot = (path: string) => {
    const body = store.get(path);
    return { id: path.split("/").at(-1), exists: body !== undefined, data: () => (body === undefined ? undefined : structuredClone(body)) };
  };
  const doc = (path: string) => ({
    id: path.split("/").at(-1),
    get: async () => snapshot(path),
    set: async (data: Body) => {
      store.set(path, structuredClone(data));
      notify();
    },
    delete: async () => {
      store.delete(path);
      notify();
    },
    onSnapshot: (next: (snap: unknown) => void) => listen(() => next(snapshot(path))),
  });
  const collection = (path: string) => {
    const query = {
      orderBy: () => query,
      limit: () => query,
      onSnapshot: (next: (snap: unknown) => void) =>
        listen(() => next({ docs: [...store.keys()].filter((key) => key.startsWith(`${path}/`)).sort().map(snapshot) })),
      doc: (id?: string) => doc(`${path}/${id ?? `q${++minted}`}`),
    };
    return query;
  };
  const db = seed.documents === null ? null : { doc, collection };
  const user = { id: async () => "u_viewer" };
  Object.assign(window, {
    claude: { use: async (name: string) => (name === "db" ? db : name === "user" ? user : null) },
    // What the tests read back, and how they stand in for an agent's sync.
    artifactStore: store,
    artifactWrite: (path: string, body: Body | null) => {
      if (body === null) store.delete(path);
      else store.set(path, body);
      notify();
    },
  });
}

/** Open the artifact page with `documents` in its database, and record every request it makes. */
async function openArtifact(page: Page, documents: Record<string, unknown> | null): Promise<string[]> {
  const requests: string[] = [];
  page.on("pageerror", (error) => {
    throw error;
  });
  page.on("request", (request) => requests.push(request.url()));
  await page.route("**/*", (route) =>
    route.request().url() === PAGE ? route.fulfill({ contentType: "text/html", body: HTML }) : route.fulfill({ status: 404, body: "" }),
  );
  await page.addInitScript(installHost, { documents });
  await page.goto(PAGE);
  return requests;
}

const queue = (page: Page) =>
  page.evaluate(() => [...(window as unknown as { artifactStore: Map<string, unknown> }).artifactStore].filter(([path]) => path.startsWith("queue/")).map(([, body]) => body));

test("the artifact renders Work, Task, Inbox and Planning from its database and queues a triage", async ({ page }) => {
  const requests = await openArtifact(page, recorded.documents);

  await expect(page.getByRole("heading", { level: 1, name: "Work" })).toBeVisible();
  await expect(page.getByText(/^Synced \w{3} \d+, \d\d:\d\d UTC\./)).toBeVisible();
  await page.getByRole("link", { name: /Sync the database/ }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Sync the database" })).toBeVisible();
  expect(new URL(page.url()).hash).toBe(`#/tasks/${BLOCKED}`);
  await expect(page.getByRole("link", { name: "Build the page" }).first()).toBeVisible();

  await page.getByRole("link", { name: "Inbox" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "Try the artifact" })).toBeVisible();
  await page.keyboard.press("Backspace");
  await page.getByRole("dialog", { name: "Discard this capture?" }).getByRole("button", { name: "Discard" }).click();
  await expect(page.getByText("Queued for the next sync")).toBeVisible();
  // The capture stays until an agent applies the edit and syncs.
  await expect(page.getByRole("button", { name: "Try the artifact" })).toBeVisible();
  await expect(page.getByText("1 edit waits for the next sync.")).toBeVisible();
  await expect(page.getByRole("list", { name: "Queued edits" })).toHaveText(`Discard the capture ${CAPTURE}`);
  expect(await queue(page)).toEqual([
    { op: "discard_capture", target: CAPTURE, input: {}, created_at: expect.any(String), created_by: "u_viewer", status: "queued" },
  ]);

  await page.getByRole("link", { name: "Planning" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Planning" })).toBeVisible();
  await expect(page.getByText("Web app").first()).toBeVisible();

  // An agent applies the edit and syncs: the capture is gone and the queue empty.
  await page.getByRole("link", { name: "Inbox" }).click();
  await expect(page.getByRole("heading", { level: 2, name: "Try the artifact" })).toBeVisible();
  await page.evaluate((capture) => {
    const host = window as unknown as { artifactStore: Map<string, Record<string, unknown>>; artifactWrite: (path: string, body: unknown) => void };
    const meta = host.artifactStore.get("meta/sync") ?? {};
    for (const path of host.artifactStore.keys()) if (path.startsWith("queue/")) host.artifactWrite(path, null);
    host.artifactWrite(`records/${capture}`, null);
    host.artifactWrite("views/captures", { format_version: 1, data: { records: [] } });
    host.artifactWrite("meta/sync", { ...meta, revision: "after-apply" });
  }, CAPTURE);
  await expect(page.getByText("The project synced")).toBeVisible();
  await expect(page.getByRole("heading", { level: 2, name: "Try the artifact" })).toBeVisible();
  await page.getByRole("button", { name: "Reload view" }).click();
  await expect(page.getByText("The Inbox is empty")).toBeVisible();
  await expect(page.getByText("The project synced")).toBeHidden();

  expect(new Set(requests)).toEqual(new Set([PAGE]));
});

test("the artifact asks to be opened in claude.ai when it has no database", async ({ page }) => {
  const requests = await openArtifact(page, null);
  await expect(page.getByRole("heading", { name: "Open this page in claude.ai" })).toBeVisible();
  expect(new Set(requests)).toEqual(new Set([PAGE]));
});
