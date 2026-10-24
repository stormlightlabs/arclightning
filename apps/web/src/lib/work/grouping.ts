import { formatStatus, type TaskGlyphKind, type TaskGroup, type TaskRowItem } from "@arclightning/ui";
import type { ReadinessReason, WorkItem, WorkList } from "$lib/data";
import { NO_MILESTONE, STATUSES, matches, type WorkFilters } from "./filters";

/**
 * Group the tasks that pass `filters` by milestone, then by status.
 *
 * Milestones keep the order of `list.milestones`, with tasks outside any
 * milestone last. Statuses follow {@link STATUSES}. Rows keep the order of
 * `list.items`, which is the CLI's. Groups and sections with no rows are left
 * out.
 */
export function groupWork(list: WorkList, filters: WorkFilters): TaskGroup[] {
  const byMilestone = new Map<string, WorkItem[]>();
  for (const item of list.items) {
    if (!matches(item, filters)) continue;
    const key = item.milestone?.id ?? NO_MILESTONE;
    byMilestone.set(key, [...(byMilestone.get(key) ?? []), item]);
  }

  const milestones = [...list.milestones];
  // A task can name a phase the phase list lacks, such as one created between
  // the two reads; it still gets a group, after the known ones.
  for (const item of list.items) {
    if (item.milestone && !milestones.some((milestone) => milestone.id === item.milestone?.id)) {
      milestones.push(item.milestone);
    }
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
    reasons: item.readiness.reasons.map((reason) => describeReason(reason, titles)),
  };
}

function count(tasks: number): string {
  return tasks === 1 ? "1 task" : `${tasks} tasks`;
}
