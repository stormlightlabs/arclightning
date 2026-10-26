/**
 * A data source over a claude.ai artifact's database, for the artifact build.
 *
 * The database holds the JSON the `arcl serve` read endpoints return, which
 * `arcl web export` writes and an agent copies in: `views/work`,
 * `views/tree`, `views/tasks`, `views/captures`, `records/<id>` for each
 * task and open capture, `sources/<id>` for each task, and `meta/sync`.
 * Views read them through the same functions as over `arcl serve`.
 *
 * The page cannot run `arcl`, so a change writes a `queue/<id>` document
 * instead and fails with {@link QUEUED}. An agent applies the queue with
 * `arcl web apply` and syncs the result back. Until then the records do not
 * change, and the page says the edit is queued. A raw record edit queues as
 * `update_source` with the revision its `sources/<id>` document held, so
 * `arcl web apply` refuses it when the file changed after that sync.
 *
 * The types here are the subset of the artifact's `db` and `user`
 * capabilities this module calls.
 */
import { type Listing, type Tree, type Work, captureOf, capturesOf, taskIndexOf, taskOf, workOf } from "./envelopes";
import {
  type Capture,
  DataError,
  type DataSource,
  QUEUED,
  type QueuedEdit,
  type RecordSource,
  type SyncStatus,
  type TaskDetail,
} from "./types";

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
  onSnapshot(
    next: (snapshot: { docs: ArtifactSnapshot[] }) => void,
    error?: (error: ArtifactDbError) => void,
  ): () => void;
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
  /** Schedule a resubscription; tests pass their own timer. */
  setTimeout?: (run: () => void, ms: number) => unknown;
  /** Cancel a scheduled resubscription. */
  clearTimeout?: (handle: unknown) => void;
}

/** What a queued change's {@link DataError} says. */
export const QUEUED_MESSAGE =
  "An agent applies this edit with arcl on the next sync. This page shows the change after that.";

/** The queued edits {@link ArtifactSource.watch} follows, at most. */
const QUEUE_LIMIT = 200;

/** The most parts a chunked document may have. `arcl web export` refuses to write more, under the same limit. */
const MAX_PARTS = 64;

/** The largest queued edit, as UTF-8 JSON. The database refuses a document over 256 KiB; the rest is headroom. */
const MAX_EDIT_BYTES = 200 * 1024;

/** The first and the longest wait before following the database again after a listener drops. */
const RETRY_FIRST_MS = 1_000;
const RETRY_MAX_MS = 60_000;

/**
 * The `onSnapshot` error codes that end a listener for good, which a fresh
 * listener cannot fix. Any other code, `unavailable` included, means the
 * platform dropped the listener and a fresh one may work.
 */
const TERMINAL_CODES = new Set([
  "invalid_argument",
  "resource_exhausted",
  "quota_exceeded",
  "revoked",
  "not_granted",
  "capability_disabled",
  "capability_removed",
  "transform_error",
]);

/** A data source over a claude.ai artifact's database. */
export function artifactSource(db: ArtifactDb, options: ArtifactOptions = {}): DataSource {
  const userId = options.userId ?? (() => Promise.resolve(null));
  const now = options.now ?? (() => new Date());
  const schedule = options.setTimeout ?? ((run, ms) => globalThis.setTimeout(run, ms));
  const cancel = options.clearTimeout ?? ((handle) => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>));

  /** Read the document at `path`, joining its parts when it is chunked. */
  async function read(path: string, signal?: AbortSignal): Promise<Record<string, unknown> | null> {
    const snapshot = await abortable(
      call(() => db.doc(path).get()),
      signal,
    );
    const body = snapshot.exists ? snapshot.data() : undefined;
    if (body === undefined) return null;
    const parts = chunkCount(path, body.chunked);
    if (parts === null) return body;
    const name = path.replace("/", "~");
    const reads = Array.from({ length: parts }, (_, index) => call(() => db.doc(`chunks/${name}~${index}`).get()));
    const texts = (await abortable(Promise.all(reads), signal)).map((part) => part.data()?.part);
    if (!texts.every((text) => typeof text === "string")) throw midSync(path);
    const text = texts.join("");
    // A sync that has written some parts but not all can leave parts that join into other JSON.
    if (isRecord(body.chunked) && body.chunked.digest !== digest(text)) throw midSync(path);
    try {
      const joined: unknown = JSON.parse(text);
      if (isRecord(joined)) return joined;
    } catch {
      // The digest matched, so the export wrote text that is not an object.
    }
    throw midSync(path);
  }

  /** The `data` of the envelope at `path`, which an export always writes. */
  async function view<T>(path: string, signal?: AbortSignal): Promise<T> {
    const body = await read(path, signal);
    if (body === null || !isRecord(body.data)) {
      throw new DataError(
        "not_synced",
        `The artifact has no \`${path}\` yet. Run \`arcl web export\` and copy its documents into the artifact.`,
      );
    }
    return body.data as T;
  }

  /** The `data` of one record's `arcl show` envelope. */
  async function record<T>(id: string, signal?: AbortSignal): Promise<T> {
    const body = await read(recordPath("records", id), signal);
    if (body === null || !isRecord(body.data))
      throw new DataError("not_found", `No record with the ID \`${id}\` was in the last sync.`);
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
    // Check the one limit an edit can break, so a refusal after this is about access.
    const size = new TextEncoder().encode(JSON.stringify(entry)).length;
    if (size > MAX_EDIT_BYTES) {
      throw new DataError(
        "too_large",
        `The edit is ${Math.ceil(size / 1024)} KiB, over the ${MAX_EDIT_BYTES / 1024} KiB the artifact queues. Make it smaller, or make it with arcl.`,
      );
    }
    await call(() => db.collection("queue").doc().set(entry)).catch((error: unknown) => {
      // The database refuses a write the viewer may not make as `invalid_argument`, with no code of its own.
      if (error instanceof DataError && error.code === "invalid_argument") {
        throw new DataError(
          "not_allowed",
          `The artifact did not queue the edit: ${error.message}. Queuing edits needs Contributor access or above.`,
        );
      }
      throw error;
    });
    throw new DataError(QUEUED, QUEUED_MESSAGE);
  }

  const queueTask =
    (op: string) =>
    (id: string, input: object = {}) =>
      queue(op, id, input);

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
    async recordSource(id, signal) {
      const path = recordPath("sources", id);
      const body = await read(path, signal);
      if (body === null || !isRecord(body.data)) {
        throw new DataError(
          "not_synced",
          `The artifact has no \`${path}\` yet. Run \`arcl web export\` and copy its documents into the artifact.`,
        );
      }
      return body.data as unknown as RecordSource;
    },
    saveRecordSource: (id, { source, revision }) => queue("update_source", id, { source, revision }),

    watch(listener) {
      const status: SyncStatus = { syncedAt: null, commit: null, revision: null, edits: [], stopped: null };
      const report = () => listener({ ...status, edits: [...status.edits] });
      let ended = false;
      const stops: (() => void)[] = [];

      /**
       * Keep a listener open with `subscribe`. When the platform drops it,
       * open a fresh one after a wait that doubles up to a minute and resets
       * once it delivers. A terminal error stops following and is reported.
       */
      function follow(subscribe: (next: () => void, fail: (error: unknown) => void) => () => void) {
        let attempt = 0;
        let timer: unknown = null;
        let unsubscribe = () => {};
        const open = () => {
          timer = null;
          unsubscribe = subscribe(
            () => (attempt = 0),
            (error) => {
              if (ended) return;
              const code = isRecord(error) && typeof error.code === "string" ? error.code : "unavailable";
              if (TERMINAL_CODES.has(code)) {
                status.stopped ??= {
                  code,
                  message: isRecord(error) && typeof error.message === "string" ? error.message : code,
                };
                report();
                return;
              }
              const wait = Math.min(RETRY_FIRST_MS * 2 ** attempt, RETRY_MAX_MS);
              attempt += 1;
              timer = schedule(open, wait / 2 + Math.random() * (wait / 2));
            },
          );
        };
        open();
        stops.push(() => {
          if (timer !== null) cancel(timer);
          unsubscribe();
        });
      }

      follow((delivered, fail) =>
        db.doc("meta/sync").onSnapshot((snapshot) => {
          delivered();
          const meta = snapshot.exists ? snapshot.data() : undefined;
          status.syncedAt = stringOr(meta?.synced_at);
          status.commit = stringOr(meta?.commit);
          status.revision = stringOr(meta?.revision);
          report();
        }, fail),
      );
      follow((delivered, fail) =>
        db
          .collection("queue")
          .orderBy("created_at")
          .limit(QUEUE_LIMIT)
          .onSnapshot((snapshot) => {
            delivered();
            status.edits = snapshot.docs.flatMap((doc) => queuedEdit(doc) ?? []);
            report();
          }, fail),
      );
      return () => {
        ended = true;
        for (const stop of stops) stop();
      };
    },

    async dismiss(id) {
      await call(() => db.doc(`queue/${id}`).delete());
    },
  };
}

/** The database path of one record's document in `collection`: `records` for its `arcl show` envelope, `sources` for its file. */
function recordPath(collection: "records" | "sources", id: string): string {
  if (!/^[A-Za-z0-9_\-.~:@+]+$/.test(id) || id === "." || id === "..")
    throw new DataError("not_found", `\`${id}\` is not a record ID.`);
  return `${collection}/${id}`;
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

/** The part count of the chunked document at `path`, or `null` for a whole document. */
function chunkCount(path: string, header: unknown): number | null {
  if (!isRecord(header)) return null;
  const parts = header.parts;
  if (typeof parts !== "number" || !Number.isInteger(parts) || parts < 1 || parts > MAX_PARTS) {
    throw new DataError(
      "bad_document",
      `\`${path}\` is split into ${String(parts)} parts; this page reads 1 to ${MAX_PARTS}. Run \`arcl web export --full\` and sync again.`,
    );
  }
  return parts;
}

/**
 * The digest `arcl web export` puts in a chunked document's header: the
 * 64-bit FNV-1a hash of the joined text's UTF-8 bytes, followed by one
 * `0xff` byte, as 16 hex digits. It tells versions apart; it is not a
 * security boundary. The hash is kept as four 16-bit limbs, so every step
 * stays exact in a JavaScript number.
 */
export function digest(text: string): string {
  // The offset basis 0xcbf29ce484222325, low limb first.
  const hash = [0x2325, 0x8422, 0x9ce4, 0xcbf2];
  const mix = (byte: number) => {
    hash[0] ^= byte;
    // Multiply by the prime 0x100000001b3: by 0x1b3, plus the hash shifted left 40 bits.
    const t0 = hash[0] * 0x1b3;
    const t1 = hash[1] * 0x1b3 + (t0 >>> 16);
    const t2 = hash[2] * 0x1b3 + (hash[0] << 8) + (t1 >>> 16);
    const t3 = hash[3] * 0x1b3 + (hash[1] << 8) + (t2 >>> 16);
    hash[0] = t0 & 0xffff;
    hash[1] = t1 & 0xffff;
    hash[2] = t2 & 0xffff;
    hash[3] = t3 & 0xffff;
  };
  for (const byte of new TextEncoder().encode(text)) mix(byte);
  mix(0xff);
  return hash
    .reverse()
    .map((limb) => limb.toString(16).padStart(4, "0"))
    .join("");
}

/** The error for a chunked document whose parts do not join. */
function midSync(path: string): DataError {
  return new DataError(
    "unavailable",
    `\`${path}\` is changing while an agent syncs the artifact. Try again in a moment.`,
  );
}

/**
 * A queued or failed edit from a `queue` document, or `null` for a document
 * that is not one. Anyone who may queue edits may write these documents, so
 * every field is checked.
 */
function queuedEdit(snapshot: ArtifactSnapshot): QueuedEdit | null {
  const body = snapshot.data();
  if (
    !body ||
    typeof body.op !== "string" ||
    (body.status !== "queued" && body.status !== "applying" && body.status !== "failed")
  )
    return null;
  const error = isRecord(body.error)
    ? { code: String(body.error.code ?? ""), message: String(body.error.message ?? "") }
    : null;
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
