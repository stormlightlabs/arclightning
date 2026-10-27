import type { Readiness, ReadinessReason, Task, TaskPriority, TaskStatus, WorkItem } from "$lib/data/types";

/** Task statuses in the order the Work list shows them. */
export const STATUSES: readonly TaskStatus[] = ["in_progress", "pending", "parked", "completed", "cancelled"];

/** Task priorities, highest first. */
export const PRIORITIES: readonly TaskPriority[] = ["critical", "high", "normal", "low"];

/** Readiness values the Work list filters on. */
export const READINESS: readonly ReadinessFilter[] = ["ready", "blocked", "waiting"];

/** The milestone filter value for tasks outside any milestone. */
export const NO_MILESTONE = "none";

/**
 * A task's readiness for filtering: `ready` can start, `blocked` waits on an
 * incomplete blocker, and `waiting` is not ready for another reason, such as
 * its status or open subtasks.
 */
export type ReadinessFilter = "ready" | "blocked" | "waiting";

/** The Work list's filters. An empty list puts no limit on its field. */
export type WorkFilters = {
  status: TaskStatus[];
  readiness: ReadinessFilter[];
  priority: TaskPriority[];
  /** Milestone IDs, or {@link NO_MILESTONE}. */
  milestone: string[];
};

/**
 * Read filters from a page URL's query. Each filter takes the first value of
 * its parameter, as its menu shows it, and drops a value the list does not
 * know. Milestone IDs are checked by {@link withKnownMilestone} once the list
 * loads.
 */
export function parseFilters(search: URLSearchParams): WorkFilters {
  const first = <T extends string>(name: string, values: readonly T[]): T[] => {
    const value = search.get(name);
    return value !== null && values.includes(value as T) ? [value as T] : [];
  };
  const milestone = search.get("milestone");
  return {
    status: first("status", STATUSES),
    readiness: first("readiness", READINESS),
    priority: first("priority", PRIORITIES),
    milestone: milestone === null || milestone === "" ? [] : [milestone],
  };
}

/** `filters` without a milestone value that is not one of `milestoneIds` or {@link NO_MILESTONE}. */
export function withKnownMilestone(filters: WorkFilters, milestoneIds: Iterable<string>): WorkFilters {
  const known = new Set([...milestoneIds, NO_MILESTONE]);
  const milestone = filters.milestone.filter((id) => known.has(id));
  return milestone.length === filters.milestone.length ? filters : { ...filters, milestone };
}

/** Write `filters` into a copy of `search`, replacing the filter parameters it had. */
export function writeFilters(filters: WorkFilters, search = new URLSearchParams()): URLSearchParams {
  const next = new URLSearchParams(search);
  for (const name of ["status", "readiness", "priority", "milestone"] as const) {
    next.delete(name);
    for (const value of filters[name]) next.append(name, value);
  }
  return next;
}

/** Whether any filter limits the list. */
export function isFiltered(filters: WorkFilters): boolean {
  return Object.values(filters).some((values) => values.length > 0);
}

/** Whether `status` closes a task: completed or cancelled. */
export function isClosed(status: TaskStatus): boolean {
  return status === "completed" || status === "cancelled";
}

/**
 * The reasons the web app shows for a task's readiness. Readiness asks
 * whether a task can start, so a closed task shows none. An open task keeps
 * every reason.
 */
export function shownReasons(task: Task, readiness: Readiness): ReadinessReason[] {
  return isClosed(task.status) ? [] : readiness.reasons;
}

/** The readiness value `item` filters as, or `null` for a closed task, which no readiness filter matches. */
export function readinessOf(item: WorkItem): ReadinessFilter | null {
  if (isClosed(item.task.status)) return null;
  if (item.readiness.ready) return "ready";
  return item.readiness.blocked ? "blocked" : "waiting";
}

/** Whether `item` passes every filter. */
export function matches(item: WorkItem, filters: WorkFilters): boolean {
  const pass = <T>(values: T[], value: T) => values.length === 0 || values.includes(value);
  return (
    pass(filters.status, item.task.status) &&
    pass(filters.readiness, readinessOf(item)) &&
    pass(filters.priority, item.task.priority) &&
    pass(filters.milestone, item.milestone?.id ?? NO_MILESTONE)
  );
}
