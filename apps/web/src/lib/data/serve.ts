import { tokenHeaders } from "$lib/token";
import {
  type Listing,
  type Tree,
  type Work,
  captureOf,
  capturesOf,
  milestoneOf,
  taskIndexOf,
  taskOf,
  workOf,
} from "./envelopes";
import {
  type Changed,
  DataError,
  type Capture,
  type DataSource,
  type Plan,
  type PlanApplied,
  type PromotionTarget,
  type RecordSource,
  type Spec,
  type Task,
  type TaskCompletion,
  type TaskDetail,
  type WorkList,
} from "./types";

/** Options for {@link serveSource}; tests pass their own. */
export type ServeOptions = {
  fetch?: typeof fetch;
  /** The headers that authenticate each request. */
  headers?: () => Record<string, string>;
};

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

  async function work(signal?: AbortSignal): Promise<WorkList> {
    return workOf(await get<Work>("/api/work", signal));
  }

  /** `post` to one capture's action. */
  function captureChange<T>(id: string, action: string, body: object): Promise<Changed<T>> {
    return post<T>(`/api/captures/${encodeURIComponent(id)}/${action}`, body);
  }

  return {
    work,

    async task(id, signal) {
      return taskOf(id, await get<Partial<TaskDetail>>(`/api/records/${encodeURIComponent(id)}`, signal));
    },

    async taskIndex() {
      return taskIndexOf(await get<Listing>("/api/records?kind=task"));
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

    async captures(signal) {
      return capturesOf(await get<Listing>("/api/records?kind=capture&status=captured", signal));
    },

    async capture(id, signal) {
      return captureOf(id, await get<{ capture?: Capture }>(`/api/records/${encodeURIComponent(id)}`, signal));
    },

    async promoteCapture(id, input) {
      // The reply names the new record by its kind: `task`, `spec` or `note`.
      const { value, warnings } = await captureChange<Partial<Record<PromotionTarget, { id: string; title: string }>>>(
        id,
        "promote",
        input,
      );
      const kind = (["task", "spec", "note"] as const).find((target) => value[target] !== undefined) ?? input.to;
      const record = value[kind];
      return { value: { kind, id: record?.id ?? "", title: record?.title ?? "" }, warnings };
    },

    async discardCapture(id) {
      const { value, warnings } = await captureChange<{ capture: Capture }>(id, "discard", {});
      return { value: value.capture, warnings };
    },

    async planning(signal) {
      const [tree, list] = await Promise.all([get<Tree>("/api/tree", signal), work(signal)]);
      return { tree: tree.nodes, work: list };
    },

    async createSpec(input) {
      const { value, warnings } = await post<{ spec: Spec }>("/api/specs", input);
      return { value: value.spec, warnings };
    },

    async createPlan(input) {
      const { value, warnings } = await post<{ plan: Plan }>("/api/plans", input);
      return { value: value.plan, warnings };
    },

    recordSource: (id, signal) => get<RecordSource>(`${recordPath(id)}/source`, signal),
    saveRecordSource: (id, edit) => post<RecordSource>(`${recordPath(id)}/edit`, edit),

    async applyPlan(id, document) {
      const { value, warnings } = await post<PlanApplied>(`/api/plans/${encodeURIComponent(id)}/apply`, { document });
      const { plan, phases, tasks } = value;
      return { value: { plan, phases: phases.map(milestoneOf), tasks }, warnings };
    },
  };
}

/** The API path of one record, with its ID escaped. */
function recordPath(id: string): string {
  return `/api/records/${encodeURIComponent(id)}`;
}

/** The API path of one task, with its ID escaped. */
function taskPath(id: string): string {
  return `/api/tasks/${encodeURIComponent(id)}`;
}
