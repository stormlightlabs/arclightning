import type { PlanningTreeNode, RecordStatus } from "@arclightning/ui";
import type { Planning, Readiness, TreeNode } from "$lib/data";

/** The keys the Planning view answers, apart from `j`, `k` and `Enter`. */
export const PLANNING_KEYS = {
  /** New spec. */
  newSpec: "n",
  /** New plan on the selected spec, or Apply plan on the selected plan. */
  add: "a",
} as const;

/** How many tasks under a record are done, and how many count. */
export interface Progress {
  /** Completed tasks. */
  done: number;
  /** Every task but the cancelled ones. */
  total: number;
}

/** A milestone row: a phase with its progress and what can start. */
export interface MilestoneRow extends Progress {
  kind: "milestone";
  id: string;
  title: string;
  status: string;
  /** Pending tasks that can start now. */
  ready: number;
  /** Pending or parked tasks waiting on an incomplete blocker. */
  blocked: number;
  /** The phase's tasks, each with its subtasks, in `arcl tree` order. */
  tree: PlanningTreeNode[];
}

/** A plan with its milestones. */
export interface PlanRow extends Progress {
  kind: "plan";
  id: string;
  title: string;
  status: string;
  milestones: MilestoneRow[];
  /** Tasks in the plan outside any phase, with their subtasks. */
  tasks: PlanningTreeNode[];
}

/** A spec with its plans. */
export interface SpecRow {
  kind: "spec";
  id: string;
  title: string;
  status: string;
  plans: PlanRow[];
}

/** A row the Planning view can select. */
export type PlanningRow = SpecRow | PlanRow | MilestoneRow;

const TASK_STATUSES: readonly string[] = ["pending", "in_progress", "parked", "completed", "cancelled"];

/**
 * The Planning view's specs, plans and milestones, in `arcl tree` order.
 * Progress and the task trees come from the tree; ready and blocked counts
 * from each task's readiness.
 */
export function planningView({ tree, work }: Planning): SpecRow[] {
  const readiness = new Map(work.items.map((item) => [item.task.id, item.readiness]));
  return tree
    .filter((node) => node.kind === "spec")
    .map((spec) => ({
      kind: "spec",
      id: spec.id,
      title: spec.title,
      status: spec.status,
      plans: spec.children.filter((node) => node.kind === "plan").map((plan) => planRow(plan, readiness)),
    }));
}

/**
 * The rows `specs` shows, in order: each spec, then its plans unless it is
 * collapsed, then each plan's milestones unless the plan is collapsed.
 */
export function visibleRows(specs: SpecRow[], collapsed: ReadonlySet<string>): PlanningRow[] {
  return specs.flatMap((spec) => [
    spec,
    ...(collapsed.has(spec.id) ? [] : spec.plans.flatMap((plan) => [plan, ...(collapsed.has(plan.id) ? [] : plan.milestones)])),
  ]);
}

/** The specs and plans that start collapsed: those no longer open. */
export function closedRecords(specs: SpecRow[]): Set<string> {
  const closed = new Set<string>();
  for (const spec of specs) {
    if (spec.status !== "open") closed.add(spec.id);
    for (const plan of spec.plans) if (plan.status !== "open") closed.add(plan.id);
  }
  return closed;
}

/** `progress` in words, such as `3 of 5 done`. */
export function describeProgress({ done, total }: Progress): string {
  return total === 0 ? "No tasks" : `${done} of ${total} done`;
}

/** A plan document `arcl plan apply` accepts, as a starting point for the form. */
export function planTemplate(): string {
  return [
    "format-version = 1",
    "",
    "[[phases]]",
    'key = "first"',
    'title = "First phase"',
    "position = 1",
    "",
    "[[phases.tasks]]",
    'key = "first-task"',
    'title = "First task"',
    "position = 1",
    "",
  ].join("\n");
}

function planRow(plan: TreeNode, readiness: Map<string, Readiness>): PlanRow {
  const milestones = plan.children.filter((node) => node.kind === "phase").map((phase) => milestoneRow(phase, readiness));
  const loose = plan.children.filter((node) => node.kind === "task");
  return {
    kind: "plan",
    id: plan.id,
    title: plan.title,
    status: plan.status,
    milestones,
    tasks: loose.map(treeNode),
    ...progressOf([...plan.children.filter((node) => node.kind === "phase").flatMap((phase) => phase.children), ...loose]),
  };
}

function milestoneRow(phase: TreeNode, readiness: Map<string, Readiness>): MilestoneRow {
  const tasks = flatten(phase.children);
  const waiting = (task: TreeNode) => task.status === "pending" || task.status === "parked";
  return {
    kind: "milestone",
    id: phase.id,
    title: phase.title,
    status: phase.status,
    ready: tasks.filter((task) => task.status === "pending" && readiness.get(task.id)?.ready).length,
    blocked: tasks.filter((task) => waiting(task) && readiness.get(task.id)?.blocked).length,
    tree: phase.children.map(treeNode),
    ...progressOf(phase.children),
  };
}

function progressOf(nodes: TreeNode[]): Progress {
  const tasks = flatten(nodes);
  return {
    done: tasks.filter((task) => task.status === "completed").length,
    total: tasks.filter((task) => task.status !== "cancelled").length,
  };
}

/** Every task in `nodes` and below them. */
function flatten(nodes: TreeNode[]): TreeNode[] {
  return nodes.flatMap((node) => (node.kind === "task" ? [node, ...flatten(node.children)] : []));
}

function treeNode(node: TreeNode): PlanningTreeNode {
  return {
    id: node.id,
    kind: "task",
    title: node.title,
    status: TASK_STATUSES.includes(node.status) ? (node.status as RecordStatus) : undefined,
    children: node.children.filter((child) => child.kind === "task").map(treeNode),
  };
}
