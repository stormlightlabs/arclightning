/**
 * A data source over a claude.ai artifact's database, for the artifact build.
 *
 * The database holds the JSON the `arcl serve` read endpoints return, which
 * `arcl web export` writes and an agent copies in: `views/work`,
 * `views/tree`, `views/tasks`, `views/captures`, `records/<id>` for each task
 * and open capture, and `meta/sync`. Views read them through the same
 * functions as over `arcl serve`.
 *
 * The page cannot run `arcl`, so a change writes a `queue/<id>` document
 * instead and fails with {@link QUEUED}. An agent applies the queue with
 * `arcl web apply` and syncs the result back. Until then the records do not
 * change, and the page says the edit is queued.
 *
 * The types here are the subset of the artifact's `db` and `user`
 * capabilities this module calls.
 */
import { type Listing, type Tree, type Work, captureOf, capturesOf, taskIndexOf, taskOf, workOf } from "./envelopes";
import { type Capture, DataError, type DataSource, QUEUED, type QueuedEdit, type SyncStatus, type TaskDetail } from "./types";

/** A failed `db` call, as the artifact rejects it. */
export interface ArtifactDbError {
  code: string;
  message: string;
}

/** One document as a read or snapshot delivers it. */
export interface ArtifactSnapshot {
  id: string;
  exists: boolean;
  data(): Record<string, unknown> | undefined;
}

/** A reference to one document. */
export interface ArtifactDocument {
  id: string;
  get(): Promise<ArtifactSnapshot>;
  set(data: Record<string, unknown>): Promise<void>;
  delete(): Promise<void>;
  onSnapshot(next: (snapshot: ArtifactSnapshot) => void, error?: (error: ArtifactDbError) => void): () => void;
}

/** A query over one collection. */
export interface ArtifactQuery {
  limit(count: number): ArtifactQuery;
  onSnapshot(next: (snapshot: { docs: ArtifactSnapshot[] }) => void, error?: (error: ArtifactDbError) => void): () => void;
}

/** A collection: a query over everything in it, plus its documents. */
export interface ArtifactCollection extends ArtifactQuery {
  /** A document; without `id`, one under a fresh ID. */
  doc(id?: string): ArtifactDocument;
  orderBy(field: string, direction?: "asc" | "desc"): ArtifactQuery;
}

/** The artifact's `db` capability. */
export interface ArtifactDb {
  doc(path: string): ArtifactDocument;
  collection(path: string): ArtifactCollection;
}

/** The artifact's `user` capability. */
export interface ArtifactUser {
  /** The viewer's ID, or `null` when the page has no identity for them. */
  id(): Promise<string | null>;
}

/** `window.claude` on an artifact page. */
export interface ArtifactHost {
  use(name: "db"): Promise<ArtifactDb | null>;
  use(name: "user"): Promise<ArtifactUser | null>;
}

/** Options for {@link artifactSource}; tests pass their own. */
export interface ArtifactOptions {
  /** The ID of the person viewing the page, stored on each queued edit. */
  userId?: () => Promise<string | null>;
  now?: () => Date;
}

/** What a queued change's {@link DataError} says. */
export const QUEUED_MESSAGE = "An agent applies this edit with arcl on the next sync. This page shows the change after that.";

/** The queued edits {@link ArtifactSource.watch} follows, at most. */
const QUEUE_LIMIT = 200;

/** The most parts a chunked document may have: 200 KiB each, well past any real document. */
const MAX_PARTS = 64;

/** A data source over a claude.ai artifact's database. */
export function artifactSource(db: ArtifactDb, options: ArtifactOptions = {}): DataSource {
  const userId = options.userId ?? (() => Promise.resolve(null));
  const now = options.now ?? (() => new Date());

  /** Read the document at `path`, joining its parts when it is chunked. */
  async function read(path: string, signal?: AbortSignal): Promise<Record<string, unknown> | null> {
    const snapshot = await abortable(call(() => db.doc(path).get()), signal);
    const body = snapshot.exists ? snapshot.data() : undefined;
    if (body === undefined) return null;
    const parts = chunkCount(body.chunked);
    if (parts === null) return body;
    const name = path.replace("/", "~");
    const reads = Array.from({ length: parts }, (_, index) => call(() => db.doc(`chunks/${name}~${index}`).get()));
    const texts = (await abortable(Promise.all(reads), signal)).map((part) => part.data()?.part);
    if (!texts.every((text) => typeof text === "string")) throw midSync(path);
    try {
      const joined: unknown = JSON.parse(texts.join(""));
      if (isRecord(joined)) return joined;
    } catch {
      // A sync that has written some parts but not all leaves text that does not parse.
    }
    throw midSync(path);
  }

  /** The `data` of the envelope at `path`, which an export always writes. */
  async function view<T>(path: string, signal?: AbortSignal): Promise<T> {
    const body = await read(path, signal);
    if (body === null || !isRecord(body.data)) {
      throw new DataError("not_synced", `The artifact has no \`${path}\` yet. Run \`arcl web export\` and copy its documents into the artifact.`);
    }
    return body.data as T;
  }

  /** The `data` of one record's `arcl show` envelope. */
  async function record<T>(id: string, signal?: AbortSignal): Promise<T> {
    const body = await read(recordPath(id), signal);
    if (body === null || !isRecord(body.data)) throw new DataError("not_found", `No record with the ID \`${id}\` was in the last sync.`);
    return body.data as T;
  }

  /** Queue `op` for an agent to apply, then fail with {@link QUEUED}. */
  async function queue(op: string, target: string | null, input: object): Promise<never> {
    const entry = {
      op,
      target,
      // Drop `undefined` fields, which the database does not store.
      input: JSON.parse(JSON.stringify(input)) as Record<string, unknown>,
      created_at: now().toISOString(),
      created_by: await userId(),
      status: "queued",
    };
    await call(() => db.collection("queue").doc().set(entry)).catch((error: unknown) => {
      if (error instanceof DataError && error.code === "invalid_argument") {
        throw new DataError("not_allowed", `The artifact did not queue the edit: ${error.message}. Queuing edits needs Contributor access or above.`);
      }
      throw error;
    });
    throw new DataError(QUEUED, QUEUED_MESSAGE);
  }

  const queueTask = (op: string) => (id: string, input: object = {}) => queue(op, id, input);

  async function work(signal?: AbortSignal) {
    return workOf(await view<Work>("views/work", signal));
  }

  return {
    work,
    async task(id, signal) {
      return taskOf(id, await record<Partial<TaskDetail>>(id, signal));
    },
    async taskIndex() {
      return taskIndexOf(await view<Listing>("views/tasks"));
    },
    async captures(signal) {
      return capturesOf(await view<Listing>("views/captures", signal));
    },
    async capture(id, signal) {
      return captureOf(id, await record<{ capture?: Capture }>(id, signal));
    },
    async planning(signal) {
      const [tree, list] = await Promise.all([view<Tree>("views/tree", signal), work(signal)]);
      return { tree: tree.nodes, work: list };
    },

    createTask: (input) => queue("create_task", null, input),
    updateTask: queueTask("update_task"),
    startTask: queueTask("start_task"),
    parkTask: (id) => queue("park_task", id, {}),
    unparkTask: (id) => queue("unpark_task", id, {}),
    handOffTask: (id, note) => queue("hand_off_task", id, { note }),
    completeTask: queueTask("complete_task"),
    createCapture: (input) => queue("create_capture", null, input),
    promoteCapture: (id, input) => queue("promote_capture", id, input),
    discardCapture: (id) => queue("discard_capture", id, {}),
    createSpec: (input) => queue("create_spec", null, input),
    createPlan: (input) => queue("create_plan", null, input),
    applyPlan: (id, document) => queue("apply_plan", id, { document }),

    watch(listener) {
      const status: SyncStatus = { syncedAt: null, commit: null, revision: null, edits: [] };
      const report = () => listener({ ...status, edits: [...status.edits] });
      const stopMeta = db.doc("meta/sync").onSnapshot((snapshot) => {
        const meta = snapshot.exists ? snapshot.data() : undefined;
        status.syncedAt = stringOr(meta?.synced_at);
        status.commit = stringOr(meta?.commit);
        status.revision = stringOr(meta?.revision);
        report();
      });
      const stopQueue = db
        .collection("queue")
        .orderBy("created_at")
        .limit(QUEUE_LIMIT)
        .onSnapshot((snapshot) => {
          status.edits = snapshot.docs.flatMap((doc) => queuedEdit(doc) ?? []);
          report();
        });
      return () => {
        stopMeta();
        stopQueue();
      };
    },

    async dismiss(id) {
      await call(() => db.doc(`queue/${id}`).delete());
    },
  };
}

/** The database path of one record's `arcl show` envelope. */
function recordPath(id: string): string {
  if (!/^[A-Za-z0-9_\-.~:@+]+$/.test(id) || id === "." || id === "..") throw new DataError("not_found", `\`${id}\` is not a record ID.`);
  return `records/${id}`;
}

/** Run a `db` call, and turn its rejection, or a malformed path, into a {@link DataError}. */
async function call<T>(run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    if (isRecord(error) && typeof error.code === "string") {
      throw new DataError(error.code, typeof error.message === "string" ? error.message : error.code);
    }
    throw new DataError("unavailable", error instanceof Error ? error.message : String(error));
  }
}

/** Settle as `promise` does, or reject with an `AbortError` once `signal` aborts. */
function abortable<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return promise;
  return new Promise<T>((resolve, reject) => {
    const abort = () => reject(new DOMException("The read was aborted.", "AbortError"));
    if (signal.aborted) return abort();
    signal.addEventListener("abort", abort, { once: true });
    promise.then(resolve, reject).finally(() => signal.removeEventListener("abort", abort));
  });
}

/** The part count of a chunked document's header, or `null` for a whole document. */
function chunkCount(header: unknown): number | null {
  if (!isRecord(header)) return null;
  const parts = header.parts;
  if (typeof parts !== "number" || !Number.isInteger(parts) || parts < 1 || parts > MAX_PARTS) return null;
  return parts;
}

/** The error for a chunked document whose parts do not join. */
function midSync(path: string): DataError {
  return new DataError("unavailable", `\`${path}\` is changing while an agent syncs the artifact. Try again in a moment.`);
}

/**
 * A queued or failed edit from a `queue` document, or `null` for a document
 * that is not one. Anyone who may queue edits may write these documents, so
 * every field is checked.
 */
function queuedEdit(snapshot: ArtifactSnapshot): QueuedEdit | null {
  const body = snapshot.data();
  if (!body || typeof body.op !== "string" || (body.status !== "queued" && body.status !== "failed")) return null;
  const error = isRecord(body.error) ? { code: String(body.error.code ?? ""), message: String(body.error.message ?? "") } : null;
  return {
    id: snapshot.id,
    op: body.op,
    target: stringOr(body.target),
    input: isRecord(body.input) ? body.input : {},
    created_at: stringOr(body.created_at) ?? "",
    status: body.status,
    error,
  };
}

function stringOr(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
