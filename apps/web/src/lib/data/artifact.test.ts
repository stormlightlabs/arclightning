import { describe, expect, it } from "vitest";
import recorded from "./artifact.fixture.json";
import type { ArtifactDb, ArtifactDbError, ArtifactDocument, ArtifactHost, ArtifactQuery, ArtifactSnapshot } from "./artifact";
import {
  DataError,
  OPEN_IN_CLAUDE,
  QUEUED,
  QUEUED_MESSAGE,
  artifactSource,
  connectArtifact,
  dataSource,
  serveSource,
  type DataSource,
  type SyncStatus,
} from "./index";

type Body = Record<string, unknown>;

const DOCUMENTS = recorded.documents as Record<string, Body>;
const TASK = "arcl-t-01M5RVP5WQ2VC67M1APSRK2ZTG";
const BLOCKED = "arcl-t-01M5RVP5WQQ1JH509T612ZZ60W";
const CAPTURE = "arcl-c-01M5RVP5XHT7D6S1NWEJ9BEEYG";
const NOW = new Date("2026-10-25T06:30:00Z");

/**
 * An in-memory store with the subset of the artifact's `db` that
 * `artifactSource` calls. Snapshots are delivered synchronously.
 */
class FakeDb implements ArtifactDb {
  readonly documents = new Map<string, Body>();
  /** The error the next `set` rejects with, if any. */
  refuseSet: ArtifactDbError | null = null;
  private listeners = new Set<() => void>();
  private minted = 0;

  constructor(documents: Record<string, Body> = DOCUMENTS) {
    for (const [path, body] of Object.entries(documents)) this.documents.set(path, structuredClone(body));
  }

  /** Write `body` at `path` as an agent's sync does, and notify listeners. */
  write(path: string, body: Body | null) {
    if (body === null) this.documents.delete(path);
    else this.documents.set(path, body);
    for (const listener of this.listeners) listener();
  }

  doc(path: string): ArtifactDocument {
    const segments = path.split("/");
    if (segments.length % 2 !== 0 || segments.some((segment) => segment === "")) throw new TypeError(`bad document path ${path}`);
    const id = segments.at(-1) ?? "";
    const snapshot = (): ArtifactSnapshot => {
      const body = this.documents.get(path);
      return { id, exists: body !== undefined, data: () => (body === undefined ? undefined : structuredClone(body)) };
    };
    return {
      id,
      get: async () => snapshot(),
      set: async (data) => {
        if (this.refuseSet) throw this.refuseSet;
        this.write(path, structuredClone(data));
      },
      delete: async () => this.write(path, null),
      onSnapshot: (next) => this.listen(() => next(snapshot())),
    };
  }

  collection(path: string) {
    const query = (field: string | null, count: number): ArtifactQuery => ({
      limit: (limit) => query(field, limit),
      onSnapshot: (next) =>
        this.listen(() => {
          const docs = [...this.documents.keys()]
            .filter((key) => key.startsWith(`${path}/`) && key.split("/").length === path.split("/").length + 1)
            .map((key) => this.doc(key))
            .map((ref) => {
              const body = this.documents.get(`${path}/${ref.id}`);
              return { id: ref.id, exists: true, data: () => body };
            })
            .sort((a, b) => String(a.data()?.[field ?? ""] ?? a.id).localeCompare(String(b.data()?.[field ?? ""] ?? b.id)))
            .slice(0, count);
          next({ docs });
        }),
    });
    return {
      ...query(null, 1000),
      orderBy: (field: string) => query(field, 1000),
      doc: (id?: string) => this.doc(`${path}/${id ?? `q${++this.minted}`}`),
    };
  }

  private listen(deliver: () => void): () => void {
    this.listeners.add(deliver);
    deliver();
    return () => this.listeners.delete(deliver);
  }
}

function source(db = new FakeDb()): DataSource {
  return artifactSource(db, { userId: async () => "u_viewer", now: () => NOW });
}

/** A fetch that answers each read endpoint with the envelope the export recorded for it. */
function recordedFetch(): typeof fetch {
  const routes: Record<string, string> = {
    "/api/work": "views/work",
    "/api/tree": "views/tree",
    "/api/records?kind=task": "views/tasks",
    "/api/records?kind=capture&status=captured": "views/captures",
  };
  return (async (input: RequestInfo | URL) => {
    const path = String(input);
    const record = path.startsWith("/api/records/") ? `records/${decodeURIComponent(path.slice("/api/records/".length))}` : routes[path];
    return new Response(JSON.stringify(DOCUMENTS[record]), { status: 200 });
  }) as typeof fetch;
}

async function failure(promise: Promise<unknown>): Promise<DataError> {
  const error = await promise.then(
    () => null,
    (error: unknown) => error,
  );
  expect(error).toBeInstanceOf(DataError);
  return error as DataError;
}

describe("artifactSource reads", () => {
  it("reads every view from the recorded documents as arcl serve does", async () => {
    const artifact = source();
    const served = serveSource({ fetch: recordedFetch(), headers: () => ({}) });
    expect(await artifact.work()).toEqual(await served.work());
    expect(await artifact.planning()).toEqual(await served.planning());
    expect(await artifact.taskIndex()).toEqual(await served.taskIndex());
    expect(await artifact.captures()).toEqual(await served.captures());
    expect(await artifact.task(BLOCKED)).toEqual(await served.task(BLOCKED));
    expect(await artifact.capture(CAPTURE)).toEqual(await served.capture(CAPTURE));
  });

  it("gives Work its milestones and readiness", async () => {
    const work = await source().work();
    expect(work.milestones.map((milestone) => milestone.title)).toEqual(["Publish"]);
    const blocked = work.items.find((item) => item.task.id === BLOCKED);
    expect(blocked?.milestone?.title).toBe("Publish");
    expect(blocked?.readiness.blocked).toBe(true);
    expect(work.items.find((item) => item.task.title === "Loose task")?.milestone).toBeNull();
  });

  it("gives Task, Inbox and Planning their records", async () => {
    const artifact = source();
    const detail = await artifact.task(BLOCKED);
    expect(detail.blockers.map((blocker) => blocker.task.id)).toEqual([TASK]);
    expect(detail.phase?.title).toBe("Publish");
    expect(await artifact.captures()).toEqual([{ id: CAPTURE, title: "Try the artifact", status: "captured" }]);
    expect((await artifact.capture(CAPTURE)).body).toBe("On claude.ai.");
    const planning = await artifact.planning();
    expect(planning.tree.map((node) => [node.kind, node.title])).toContainEqual(["spec", "Web app"]);
  });

  it("refuses a record that is missing, the wrong kind or not an ID", async () => {
    const artifact = source();
    expect((await failure(artifact.task("arcl-t-00000000000000000000000000"))).code).toBe("not_found");
    expect((await failure(artifact.task(CAPTURE))).code).toBe("not_a_task");
    expect((await failure(artifact.capture(TASK))).code).toBe("not_a_capture");
    expect((await failure(artifact.task("../views/work"))).code).toBe("not_found");
  });

  it("says when a view was never synced", async () => {
    const empty = source(new FakeDb({}));
    const error = await failure(empty.work());
    expect(error.code).toBe("not_synced");
    expect(error.message).toContain("arcl web export");
  });

  it("joins a chunked document's parts, and says when a sync is half written", async () => {
    const db = new FakeDb();
    const text = JSON.stringify(DOCUMENTS[`records/${BLOCKED}`]);
    const middle = Math.floor(text.length / 2);
    db.write(`chunks/records~${BLOCKED}~0`, { part: text.slice(0, middle) });
    db.write(`chunks/records~${BLOCKED}~1`, { part: text.slice(middle) });
    db.write(`records/${BLOCKED}`, { chunked: { parts: 2, digest: "0" } });
    expect(await source(db).task(BLOCKED)).toEqual(await source().task(BLOCKED));

    db.write(`chunks/records~${BLOCKED}~1`, null);
    expect((await failure(source(db).task(BLOCKED))).code).toBe("unavailable");
  });

  it("stops a read when its signal aborts", async () => {
    const controller = new AbortController();
    controller.abort();
    await expect(source().work(controller.signal)).rejects.toMatchObject({ name: "AbortError" });
  });
});

describe("artifactSource changes", () => {
  it("queues each change instead of saving it, and leaves the records alone", async () => {
    const db = new FakeDb();
    const artifact = source(db);
    const before = structuredClone(Object.fromEntries(db.documents));
    const changes: [Promise<unknown>, string, string | null, Body][] = [
      [artifact.createTask({ title: "New", priority: "high" }), "create_task", null, { title: "New", priority: "high" }],
      [artifact.updateTask(TASK, { title: "Renamed", body: undefined }), "update_task", TASK, { title: "Renamed" }],
      [artifact.startTask(TASK, { force: true }), "start_task", TASK, { force: true }],
      [artifact.parkTask(TASK), "park_task", TASK, {}],
      [artifact.unparkTask(TASK), "unpark_task", TASK, {}],
      [artifact.handOffTask(TASK, "Resume here"), "hand_off_task", TASK, { note: "Resume here" }],
      [artifact.completeTask(TASK, { evidence: "Done." }), "complete_task", TASK, { evidence: "Done." }],
      [artifact.createCapture({ title: "Idea" }), "create_capture", null, { title: "Idea" }],
      [artifact.promoteCapture(CAPTURE, { to: "task", priority: "low" }), "promote_capture", CAPTURE, { to: "task", priority: "low" }],
      [artifact.discardCapture(CAPTURE), "discard_capture", CAPTURE, {}],
      [artifact.createSpec({ title: "Spec" }), "create_spec", null, { title: "Spec" }],
      [artifact.createPlan({ title: "Plan", spec: "arcl-s-1" }), "create_plan", null, { title: "Plan", spec: "arcl-s-1" }],
      [artifact.applyPlan("arcl-pl-1", "format-version = 1"), "apply_plan", "arcl-pl-1", { document: "format-version = 1" }],
    ];
    const queued = [];
    for (const [change, op, target, input] of changes) {
      const error = await failure(change);
      expect([error.code, error.message]).toEqual([QUEUED, QUEUED_MESSAGE]);
      queued.push({ op, target, input, created_at: NOW.toISOString(), created_by: "u_viewer", status: "queued" });
    }
    const queue = [...db.documents].filter(([path]) => path.startsWith("queue/")).map(([, body]) => body);
    expect(queue).toEqual(queued);
    for (const [path, body] of Object.entries(before)) expect(db.documents.get(path)).toEqual(body);
  });

  it("says when the artifact refuses to queue an edit", async () => {
    const db = new FakeDb();
    db.refuseSet = { code: "invalid_argument", message: "write denied" };
    const error = await failure(source(db).discardCapture(CAPTURE));
    expect(error.code).toBe("not_allowed");
    expect(error.message).toContain("Contributor");
    db.refuseSet = { code: "quota_exceeded", message: "the database is full" };
    expect((await failure(source(db).discardCapture(CAPTURE))).code).toBe("quota_exceeded");
  });
});

describe("artifactSource.watch", () => {
  it("follows the last sync and the queued and failed edits, and dismisses one", async () => {
    const db = new FakeDb();
    const artifact = source(db);
    const seen: SyncStatus[] = [];
    const stop = artifact.watch?.((status) => seen.push(status));
    expect(seen.at(-1)).toMatchObject({ syncedAt: DOCUMENTS["meta/sync"].synced_at, revision: DOCUMENTS["meta/sync"].revision, edits: [] });

    await failure(artifact.discardCapture(CAPTURE));
    db.write("queue/zz-failed", { op: "park_task", target: TASK, input: {}, created_at: "2026-10-25T07:00:00Z", status: "failed", error: { code: "conflict", message: "already parked" } });
    db.write("queue/zz-applied", { op: "park_task", target: TASK, input: {}, created_at: "2026-10-25T07:00:00Z", status: "applied" });
    db.write("queue/zz-junk", { op: 7, status: "queued" });
    expect(seen.at(-1)?.edits).toEqual([
      { id: "q1", op: "discard_capture", target: CAPTURE, input: {}, created_at: NOW.toISOString(), status: "queued", error: null },
      { id: "zz-failed", op: "park_task", target: TASK, input: {}, created_at: "2026-10-25T07:00:00Z", status: "failed", error: { code: "conflict", message: "already parked" } },
    ]);

    db.write("meta/sync", { ...DOCUMENTS["meta/sync"], revision: "next" });
    expect(seen.at(-1)?.revision).toBe("next");

    await artifact.dismiss?.("zz-failed");
    expect(db.documents.has("queue/zz-failed")).toBe(false);
    const count = seen.length;
    stop?.();
    db.write("meta/sync", { ...DOCUMENTS["meta/sync"], revision: "later" });
    expect(seen.length).toBe(count);
  });
});

describe("connectArtifact", () => {
  it("reads nothing without the artifact's database", async () => {
    const hosts: (ArtifactHost | undefined)[] = [undefined, { use: async () => null } as ArtifactHost, { use: () => Promise.reject(new Error("not granted")) } as ArtifactHost];
    for (const host of hosts) {
      expect(await connectArtifact(host)).toBe(false);
      const error = await failure(dataSource().work());
      expect([error.code, error.message]).toEqual(["unavailable", OPEN_IN_CLAUDE]);
      await failure(dataSource().discardCapture(CAPTURE));
      expect(dataSource().watch).toBeUndefined();
    }
  });

  it("reads the database and stamps queued edits with the viewer's ID", async () => {
    const db = new FakeDb();
    const host = {
      use: async (name: string) => (name === "db" ? db : { id: async () => "u_person" }),
    } as ArtifactHost;
    expect(await connectArtifact(host)).toBe(true);
    expect((await dataSource().captures()).map((capture) => capture.id)).toEqual([CAPTURE]);
    await failure(dataSource().discardCapture(CAPTURE));
    expect([...db.documents].find(([path]) => path.startsWith("queue/"))?.[1]).toMatchObject({ created_by: "u_person" });
  });
});
