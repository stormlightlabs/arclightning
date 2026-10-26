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

// The IDs sort opposite to the positions, so a test that passes on ID order fails.
export const agents: Milestone = {
  id: "ph-5",
  plan_id: "pl-1",
  title: "Milestone 5: Agent interface",
  status: "open",
  position: 5,
};
export const web: Milestone = {
  id: "ph-10",
  plan_id: "pl-1",
  title: "Milestone 7: Local web app",
  status: "open",
  position: 7,
};

/** A ready, pending task. */
export function ready(id: string, milestone: Milestone | null, fields: Partial<Task> = {}): WorkItem {
  return { task: task(id, fields), readiness: { ready: true, blocked: false, reasons: [] }, milestone };
}

/** A pending task blocked by `blocker`. */
export function blocked(
  id: string,
  blocker: string,
  milestone: Milestone | null,
  fields: Partial<Task> = {},
): WorkItem {
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
export function closed(
  id: string,
  status: Task["status"],
  milestone: Milestone | null,
  fields: Partial<Task> = {},
): WorkItem {
  return {
    task: task(id, { status, ...fields }),
    readiness: {
      ready: false,
      blocked: false,
      reasons: [{ kind: "status", message: `task status is \`${status}\``, task: id }],
    },
    milestone,
  };
}

/**
 * Tasks across two milestones and none. The lists come in ID order, which
 * differs from `arcl tree` order: `t-8` is a subtask of `t-3`, and positions
 * put `t-3` before `t-7` before `t-2`.
 */
export function sampleList(): WorkList {
  return {
    milestones: [web, agents],
    items: [
      closed("t-1", "completed", web),
      ready("t-2", agents, { priority: "high", position: 2 }),
      blocked("t-3", "t-2", agents, { position: 0 }),
      closed("t-4", "in_progress", agents),
      ready("t-5", null),
      closed("t-6", "parked", agents),
      ready("t-7", agents, { position: 1 }),
      ready("t-8", agents, { parent_id: "t-3", position: 0 }),
    ],
  };
}
