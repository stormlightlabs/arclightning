import { formatStatus, type TaskGlyphKind, type TaskGroup, type TaskRowItem } from "@arclightning/ui";
import type { Milestone, ReadinessReason, WorkItem, WorkList } from "$lib/data";
import { NO_MILESTONE, STATUSES, matches, shownReasons, type WorkFilters } from "./filters";

/**
 * Group the tasks that pass `filters` by milestone, then by status.
 *
 * Milestones follow {@link orderMilestones}, with tasks outside any milestone
 * last. Statuses follow {@link STATUSES}. Rows follow {@link orderTasks}.
 * Groups and sections with no rows are left out.
 */
export function groupWork(list: WorkList, filters: WorkFilters): TaskGroup[] {
  const milestones = orderMilestones(list);
  const byMilestone = new Map<string, WorkItem[]>();
  for (const item of orderTasks(list.items, milestones)) {
    if (!matches(item, filters)) continue;
    const key = item.milestone?.id ?? NO_MILESTONE;
    byMilestone.set(key, [...(byMilestone.get(key) ?? []), item]);
  }

  const titles = new Map([
    ...milestones.map((milestone): [string, string] => [milestone.id, milestone.title]),
    ...list.items.map((item): [string, string] => [item.task.id, item.task.title]),
  ]);
  const order = [...milestones.map(({ id, title }) => ({ id, title })), { id: NO_MILESTONE, title: "No milestone" }];

  return order.flatMap(({ id, title }) => {
    const items = byMilestone.get(id);
    if (!items) return [];
    const sections = STATUSES.flatMap((status) => {
      const rows = items.filter((item) => item.task.status === status).map((item) => toRow(item, titles));
      return rows.length ? [{ id: `${id}/${status}`, title: formatStatus(status), rows }] : [];
    });
    return [{ id, title, detail: count(items.length), sections }];
  });
}

/**
 * Every milestone in `list`, in `arcl tree` order: by plan, then position,
 * then ID. A phase that only a task names, such as one created between the
 * two reads, is included.
 */
export function orderMilestones(list: WorkList): Milestone[] {
  const byId = new Map(list.milestones.map((milestone) => [milestone.id, milestone]));
  for (const { milestone } of list.items) {
    if (milestone && !byId.has(milestone.id)) byId.set(milestone.id, milestone);
  }
  return [...byId.values()].sort(
    (left, right) =>
      compare(left.plan_id, right.plan_id) || left.position - right.position || compare(left.id, right.id),
  );
}

/**
 * `items` in `arcl tree` order: top-level tasks by milestone in `milestones`
 * order, then position, then ID, each followed by its subtasks in the same
 * order. Tasks outside any milestone come last. A subtask whose parent is not
 * in `items` sorts as a top-level task.
 */
export function orderTasks(items: WorkItem[], milestones: Milestone[]): WorkItem[] {
  const rank = new Map(milestones.map((milestone, index) => [milestone.id, index]));
  const rankOf = (item: WorkItem) =>
    item.milestone ? (rank.get(item.milestone.id) ?? milestones.length) : milestones.length + 1;
  const byPosition = (left: WorkItem, right: WorkItem) =>
    left.task.position - right.task.position || compare(left.task.id, right.task.id);

  const ids = new Set(items.map((item) => item.task.id));
  const children = new Map<string, WorkItem[]>();
  const roots: WorkItem[] = [];
  for (const item of items) {
    const parent = item.task.parent_id;
    if (parent !== null && ids.has(parent) && parent !== item.task.id) {
      children.set(parent, [...(children.get(parent) ?? []), item]);
    } else {
      roots.push(item);
    }
  }
  roots.sort((left, right) => rankOf(left) - rankOf(right) || byPosition(left, right));

  const ordered: WorkItem[] = [];
  const seen = new Set<string>();
  const visit = (item: WorkItem) => {
    if (seen.has(item.task.id)) return;
    seen.add(item.task.id);
    ordered.push(item);
    for (const child of (children.get(item.task.id) ?? []).sort(byPosition)) visit(child);
  };
  roots.forEach(visit);
  // A parent cycle leaves its tasks unreached from any root; keep them, by position.
  items
    .filter((item) => !seen.has(item.task.id))
    .sort(byPosition)
    .forEach(visit);
  return ordered;
}

/** The glyph for `item`: its status once started or closed, else its readiness. */
export function glyphOf(item: WorkItem): TaskGlyphKind {
  switch (item.task.status) {
    case "in_progress":
      return "in_progress";
    case "completed":
      return "done";
    case "cancelled":
      return "cancelled";
    default:
      if (item.readiness.ready) return "ready";
      return item.readiness.blocked ? "blocked" : "waiting";
  }
}

/** A reason's message with the record IDs it quotes replaced by their titles. */
export function describeReason(reason: ReadinessReason, titles: Map<string, string>): string {
  return reason.message.replace(/`([^`]+)`/g, (_, id: string) => titles.get(id) ?? id);
}

function toRow(item: WorkItem, titles: Map<string, string>): TaskRowItem {
  return {
    id: item.task.id,
    title: item.task.title,
    status: item.task.status,
    priority: item.task.priority,
    glyph: glyphOf(item),
    ready: item.readiness.ready,
    reasons: shownReasons(item.task, item.readiness).map((reason) => describeReason(reason, titles)),
  };
}

/** Compare two IDs by code unit, as Rust's `str` ordering does. */
function compare(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function count(tasks: number): string {
  return tasks === 1 ? "1 task" : `${tasks} tasks`;
}
