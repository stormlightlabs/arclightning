import { tokenHeaders } from "$lib/token";
import {
  DataError,
  type DataSource,
  type Milestone,
  type Readiness,
  type Task,
  type TaskCompletion,
  type TaskDetail,
  type WorkItem,
} from "./types";

/** How many explain requests the Work list keeps in flight at once. */
const EXPLAIN_CONCURRENCY = 4;

/** Options for {@link serveSource}; tests pass their own. */
export interface ServeOptions {
  fetch?: typeof fetch;
  /** The headers that authenticate each request. */
  headers?: () => Record<string, string>;
}

/** A row of `arcl list --json`. */
interface ListedRecord {
  id: string;
  kind: string;
  status: string;
  title: string;
}

/** A phase as `arcl explain --json` and `arcl show --json` return it. */
interface Phase {
  id: string;
  plan_id: string;
  title: string;
  status: Milestone["status"];
  position: number;
}

/** The parts of `arcl explain --json` the Work list reads. */
interface Explanation {
  task: Task;
  readiness: Readiness;
  phase: Phase | null;
}

/**
 * A data source over the `arcl serve` that served the page.
 *
 * A task or phase deleted between the list and the read that follows it is
 * left out; any other failed read fails the whole list. Changes go out as
 * POSTs with a JSON body; the browser adds the `Origin` header the server
 * checks.
 */
export function serveSource(options: ServeOptions = {}): DataSource {
  const request = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  const headers = options.headers ?? (() => tokenHeaders(window.sessionStorage));

  async function get<T>(path: string): Promise<T> {
    return send<T>(path, { headers: headers() });
  }

  async function post<T>(path: string, body: object): Promise<T> {
    return send<T>(path, {
      method: "POST",
      headers: { ...headers(), "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  /** `post` to one task's action, answering with the task it changed. */
  async function change(id: string, action: string, body: object = {}): Promise<Task> {
    const data = await post<{ task: Task }>(`${taskPath(id)}/${action}`, body);
    return data.task;
  }

  async function send<T>(path: string, init: RequestInit): Promise<T> {
    const response = await request(path, init);
    const envelope = await response.json().catch(() => null);
    if (!response.ok || envelope === null || !("data" in envelope)) {
      const error = envelope?.error;
      throw new DataError(error?.code ?? "unavailable", error?.message ?? `${path} answered ${response.status}`);
    }
    return envelope.data as T;
  }

  /** `get(path)`, or `null` when the record it names no longer exists. */
  async function getIfFound<T>(path: string): Promise<T | null> {
    try {
      return await get<T>(path);
    } catch (error) {
      if (error instanceof DataError && error.code === "not_found") return null;
      throw error;
    }
  }

  return {
    async work() {
      const [tasks, phases] = await Promise.all([
        get<{ records: ListedRecord[] }>("/api/records?kind=task"),
        get<{ records: ListedRecord[] }>("/api/records?kind=phase"),
      ]);
      const explanations = (
        await mapLimit(tasks.records, EXPLAIN_CONCURRENCY, (record) =>
          getIfFound<Explanation>(`/api/tasks/${encodeURIComponent(record.id)}/explain`),
        )
      ).filter((explanation) => explanation !== null);

      // Explain carries each phase's plan and position; show the phases no task names.
      const known = new Map<string, Milestone>();
      for (const { phase } of explanations) if (phase !== null) known.set(phase.id, milestoneOf(phase));
      const missing = phases.records.filter((record) => !known.has(record.id));
      const shown = await mapLimit(missing, EXPLAIN_CONCURRENCY, (record) =>
        getIfFound<{ phase: Phase }>(`/api/records/${encodeURIComponent(record.id)}`),
      );
      for (const found of shown) if (found !== null) known.set(found.phase.id, milestoneOf(found.phase));

      const milestones = phases.records.flatMap((record) => known.get(record.id) ?? []);
      const items = explanations.map(
        ({ task, readiness, phase }): WorkItem => ({
          task,
          readiness,
          milestone: phase === null ? null : milestoneOf(phase),
        }),
      );
      return { milestones, items };
    },

    async task(id) {
      const detail = await get<Partial<TaskDetail>>(`/api/records/${encodeURIComponent(id)}`);
      // `arcl show` answers for any record kind; only a task has readiness.
      if (!("readiness" in detail)) throw new DataError("not_a_task", `\`${id}\` is not a task`);
      return detail as TaskDetail;
    },

    async createTask(input) {
      return (await post<{ task: Task }>("/api/tasks", input)).task;
    },
    updateTask: (id, edit) => change(id, "update", edit),
    startTask: (id, options = {}) => change(id, "start", options),
    parkTask: (id) => change(id, "park"),
    unparkTask: (id) => change(id, "unpark"),
    handOffTask: (id, note) => change(id, "handoff", { note }),
    async completeTask(id, options = {}) {
      const { task, completed_subtasks, unblocked } = await post<TaskCompletion>(`${taskPath(id)}/complete`, options);
      return { task, completed_subtasks, unblocked };
    },
  };
}

/** The API path of one task, with its ID escaped. */
function taskPath(id: string): string {
  return `/api/tasks/${encodeURIComponent(id)}`;
}

function milestoneOf({ id, plan_id, title, status, position }: Phase): Milestone {
  return { id, plan_id, title, status, position };
}

/** Map `items` through `run` with at most `limit` calls pending, keeping order. */
async function mapLimit<T, R>(items: T[], limit: number, run: (item: T) => Promise<R>): Promise<R[]> {
  const results = new Array<R>(items.length);
  let next = 0;
  async function worker() {
    while (next < items.length) {
      const index = next++;
      results[index] = await run(items[index]);
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return results;
}
