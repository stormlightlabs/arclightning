import type { Milestone, Task, WorkItem, WorkList } from "$lib/data";

/** A task with defaults for the fields a test does not care about. */
export function task(id: string, fields: Partial<Task> = {}): Task {
  return {
    id,
    spec_id: null,
    plan_id: null,
    phase_id: null,
    parent_id: null,
    title: `Task ${id}`,
    body: "",
    status: "pending",
    priority: "normal",
    position: 0,
    handoff: "",
    evidence: "",
    ...fields,
  };
}

export const agents: Milestone = { id: "ph-5", title: "Milestone 5: Agent interface", status: "open" };
export const web: Milestone = { id: "ph-7", title: "Milestone 7: Local web app", status: "open" };

/** A ready, pending task. */
export function ready(id: string, milestone: Milestone | null, fields: Partial<Task> = {}): WorkItem {
  return { task: task(id, fields), readiness: { ready: true, blocked: false, reasons: [] }, milestone };
}

/** A pending task blocked by `blocker`. */
export function blocked(id: string, blocker: string, milestone: Milestone | null, fields: Partial<Task> = {}): WorkItem {
  return {
    task: task(id, fields),
    readiness: {
      ready: false,
      blocked: true,
      reasons: [{ kind: "direct_blocker", message: `blocked by \`${blocker}\` (pending)`, blocker }],
    },
    milestone,
  };
}

/** A task that is not ready because of its status. */
export function closed(id: string, status: Task["status"], milestone: Milestone | null, fields: Partial<Task> = {}): WorkItem {
  return {
    task: task(id, { status, ...fields }),
    readiness: { ready: false, blocked: false, reasons: [{ kind: "status", message: `task status is \`${status}\``, task: id }] },
    milestone,
  };
}

/** Tasks across two milestones and none, in the CLI's order. */
export function sampleList(): WorkList {
  return {
    milestones: [agents, web],
    items: [
      closed("t-1", "completed", web),
      ready("t-2", agents, { priority: "high" }),
      blocked("t-3", "t-2", agents),
      closed("t-4", "in_progress", agents),
      ready("t-5", null),
      closed("t-6", "parked", agents),
      ready("t-7", agents),
    ],
  };
}
