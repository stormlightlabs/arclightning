import type { Planning, Readiness, Task, TreeNode, WorkItem } from "$lib/data";

/** A tree node of `kind` with `children`. */
export function node(kind: string, id: string, status: string, children: TreeNode[] = []): TreeNode {
  return { kind, id, title: `${kind} ${id}`, status, children };
}

function item(id: string, status: Task["status"], readiness: Readiness): WorkItem {
  const task: Task = {
    id,
    spec_id: null,
    plan_id: null,
    phase_id: null,
    parent_id: null,
    title: `task ${id}`,
    body: "",
    status,
    priority: "normal",
    position: 0,
    handoff: "",
    evidence: "",
  };
  return { task, readiness, milestone: null };
}

const ready: Readiness = { ready: true, blocked: false, reasons: [] };
const blocked: Readiness = { ready: false, blocked: true, reasons: [{ kind: "direct_blocker", message: "blocked" }] };
const waiting: Readiness = { ready: false, blocked: false, reasons: [{ kind: "status", message: "started" }] };

/**
 * One open spec with an open plan of two phases, a closed spec with a closed
 * plan, and a spec with no plan. Phase `ph-1` has a done task, a ready task
 * with a blocked subtask, a cancelled task and an in-progress task.
 */
export function planning(): Planning {
  const tree = [
    node("capture", "c-1", "captured"),
    node("spec", "s-1", "open", [
      node("plan", "pl-1", "open", [
        node("phase", "ph-1", "open", [
          node("task", "t-1", "completed"),
          node("task", "t-2", "pending", [node("task", "t-3", "pending")]),
          node("task", "t-4", "cancelled"),
          node("task", "t-5", "in_progress"),
        ]),
        node("phase", "ph-2", "completed", [node("task", "t-6", "completed")]),
        node("task", "t-7", "pending"),
      ]),
    ]),
    node("spec", "s-2", "completed", [node("plan", "pl-2", "completed", [])]),
    node("spec", "s-3", "open"),
    node("task", "t-8", "pending"),
  ];
  const items = [
    item("t-1", "completed", waiting),
    item("t-2", "pending", ready),
    item("t-3", "pending", blocked),
    item("t-4", "cancelled", waiting),
    item("t-5", "in_progress", waiting),
    item("t-6", "completed", waiting),
    item("t-7", "pending", ready),
    item("t-8", "pending", ready),
  ];
  return { tree, work: { milestones: [], items } };
}
