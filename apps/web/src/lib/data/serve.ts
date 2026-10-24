import { tokenHeaders } from "$lib/token";
import { DataError, type DataSource, type Milestone, type Readiness, type Task, type TaskDetail, type WorkItem } from "./types";

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

/** The parts of `arcl explain --json` the Work list reads. */
interface Explanation {
  task: Task;
  readiness: Readiness;
  phase: Milestone | null;
}

/** A data source that reads from the `arcl serve` that served the page. */
export function serveSource(options: ServeOptions = {}): DataSource {
  const request = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  const headers = options.headers ?? (() => tokenHeaders(window.sessionStorage));

  async function get<T>(path: string): Promise<T> {
    const response = await request(path, { headers: headers() });
    const envelope = await response.json().catch(() => null);
    if (!response.ok || envelope === null || !("data" in envelope)) {
      const error = envelope?.error;
      throw new DataError(error?.code ?? "unavailable", error?.message ?? `${path} answered ${response.status}`);
    }
    return envelope.data as T;
  }

  return {
    async work() {
      const [tasks, phases] = await Promise.all([
        get<{ records: ListedRecord[] }>("/api/records?kind=task"),
        get<{ records: ListedRecord[] }>("/api/records?kind=phase"),
      ]);
      const explanations = await mapLimit(tasks.records, EXPLAIN_CONCURRENCY, (record) =>
        get<Explanation>(`/api/tasks/${encodeURIComponent(record.id)}/explain`),
      );
      const milestones = phases.records.map(({ id, title, status }) => ({ id, title, status }) as Milestone);
      const items = explanations.map(
        ({ task, readiness, phase }): WorkItem => ({
          task,
          readiness,
          milestone: phase === null ? null : { id: phase.id, title: phase.title, status: phase.status },
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
  };
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
