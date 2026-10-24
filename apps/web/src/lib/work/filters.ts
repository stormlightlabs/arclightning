import type { TaskPriority, TaskStatus, WorkItem } from "$lib/data";

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
export interface WorkFilters {
  status: TaskStatus[];
  readiness: ReadinessFilter[];
  priority: TaskPriority[];
  /** Milestone IDs, or {@link NO_MILESTONE}. */
  milestone: string[];
}

/** Read filters from a page URL's query, dropping values the list does not know. */
export function parseFilters(search: URLSearchParams): WorkFilters {
  const known = <T extends string>(name: string, values: readonly T[]) =>
    search.getAll(name).filter((value): value is T => values.includes(value as T));
  return {
    status: known("status", STATUSES),
    readiness: known("readiness", READINESS),
    priority: known("priority", PRIORITIES),
    milestone: search.getAll("milestone").filter((value) => value !== ""),
  };
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

/** The readiness value `item` filters as. */
export function readinessOf(item: WorkItem): ReadinessFilter {
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
