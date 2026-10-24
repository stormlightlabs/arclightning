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

/**
 * A data source over the `arcl serve` that served the page.
 *
 * Changes go out as POSTs with a JSON body; the browser adds the `Origin`
 * header the server checks.
 */
export function serveSource(options: ServeOptions = {}): DataSource {
  const request = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  const headers = options.headers ?? (() => tokenHeaders(window.sessionStorage));

  async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
    return send<T>(path, { headers: headers(), signal });
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

    async task(id, signal) {
      const detail = await get<Partial<TaskDetail>>(`/api/records/${encodeURIComponent(id)}`, signal);
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
