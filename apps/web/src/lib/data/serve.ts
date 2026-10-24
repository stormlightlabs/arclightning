import { tokenHeaders } from "$lib/token";
import { DataError, type DataSource, type Milestone, type Readiness, type Task, type WorkItem } from "./types";

/** Options for {@link serveSource}; tests pass their own. */
export interface ServeOptions {
  fetch?: typeof fetch;
  /** The headers that authenticate each request. */
  headers?: () => Record<string, string>;
}

/** A phase as `arcl serve` returns it. */
interface Phase {
  id: string;
  plan_id: string;
  title: string;
  status: Milestone["status"];
  position: number;
}

/** The `data` of `/api/work`: every task with its readiness and phase, and every phase. */
interface Work {
  tasks: { task: Task; readiness: Readiness; phase_id: string | null }[];
  phases: Phase[];
}

/** A data source that reads from the `arcl serve` that served the page. */
export function serveSource(options: ServeOptions = {}): DataSource {
  const request = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  const headers = options.headers ?? (() => tokenHeaders(window.sessionStorage));

  async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
    const response = await request(path, { headers: headers(), signal });
    const envelope = await response.json().catch(() => null);
    if (!response.ok || envelope === null || !("data" in envelope)) {
      const error = envelope?.error;
      throw new DataError(error?.code ?? "unavailable", error?.message ?? `${path} answered ${response.status}`);
    }
    return envelope.data as T;
  }

  return {
    async work(signal) {
      const work = await get<Work>("/api/work", signal);
      const milestones = work.phases.map(milestoneOf);
      const byId = new Map(milestones.map((milestone) => [milestone.id, milestone]));
      const items = work.tasks.map(
        ({ task, readiness, phase_id }): WorkItem => ({
          task,
          readiness,
          milestone: phase_id === null ? null : (byId.get(phase_id) ?? null),
        }),
      );
      return { milestones, items };
    },
  };
}

function milestoneOf({ id, plan_id, title, status, position }: Phase): Milestone {
  return { id, plan_id, title, status, position };
}
