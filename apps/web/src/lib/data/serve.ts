import { tokenHeaders } from "$lib/token";
import {
  type Changed,
  DataError,
  type Capture,
  type DataSource,
  type Milestone,
  type Readiness,
  type Task,
  type TaskCompletion,
  type TaskDetail,
  type TaskStatus,
  type WorkItem,
} from "./types";

/** Options for {@link serveSource}; tests pass their own. */
export interface ServeOptions {
  fetch?: typeof fetch;
  /** The headers that authenticate each request. */
  headers?: () => Record<string, string>;
}

/** A record as `arcl list --json` lists it. */
interface ListedRecord {
  id: string;
  kind: string;
  status: string;
  title: string;
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
 * header the server checks. A change's reply may list sync warnings, which
 * come back with its result.
 */
export function serveSource(options: ServeOptions = {}): DataSource {
  const request = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
  const headers = options.headers ?? (() => tokenHeaders(window.sessionStorage));

  async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
    return (await send<T>(path, { headers: headers(), signal })).value;
  }

  async function post<T>(path: string, body: object): Promise<Changed<T>> {
    return send<T>(path, {
      method: "POST",
      headers: { ...headers(), "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  /** `post` to one task's action, answering with the task it changed. */
  async function change(id: string, action: string, body: object = {}): Promise<Changed<Task>> {
    const { value, warnings } = await post<{ task: Task }>(`${taskPath(id)}/${action}`, body);
    return { value: value.task, warnings };
  }

  /** Send a request, and return the envelope's `data` with its `warnings`. */
  async function send<T>(path: string, init: RequestInit): Promise<Changed<T>> {
    const response = await request(path, init);
    const envelope = await response.json().catch(() => null);
    if (!response.ok || envelope === null || !("data" in envelope)) {
      const error = envelope?.error;
      throw new DataError(error?.code ?? "unavailable", error?.message ?? `${path} answered ${response.status}`);
    }
    return { value: envelope.data as T, warnings: Array.isArray(envelope.warnings) ? envelope.warnings : [] };
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

    async taskIndex() {
      const { records } = await get<{ records: ListedRecord[] }>("/api/records?kind=task");
      return records.map(({ id, title, status }) => ({ id, title, status: status as TaskStatus }));
    },

    async createTask(input) {
      const { value, warnings } = await post<{ task: Task }>("/api/tasks", input);
      return { value: value.task, warnings };
    },
    updateTask: (id, edit) => change(id, "update", edit),
    startTask: (id, options = {}) => change(id, "start", options),
    parkTask: (id) => change(id, "park"),
    unparkTask: (id) => change(id, "unpark"),
    handOffTask: (id, note) => change(id, "handoff", { note }),
    async completeTask(id, options = {}) {
      const { value, warnings } = await post<TaskCompletion>(`${taskPath(id)}/complete`, options);
      const { task, completed_subtasks, unblocked } = value;
      return { value: { task, completed_subtasks, unblocked }, warnings };
    },
    async createCapture(input) {
      const { value, warnings } = await post<{ capture: Capture }>("/api/captures", input);
      return { value: value.capture, warnings };
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
