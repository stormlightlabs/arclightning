import { formatStatus, type PropertyItem } from "@arclightning/ui";
import type { InheritedBlocker, ReadinessReason, Task, TaskDetail } from "$lib/data";
import { inField, taskHref, type WorkKey } from "$lib/work";

/** What a key does in the Task view. */
export type TaskKeyAction = "back" | "properties";

/** A run of a readiness reason's text: plain, or a record it names. */
export type ReasonSegment = { text: string } | { id: string; label: string; href: string };

/** The inherited blockers one ancestor holds. */
export interface InheritedGroup {
  ancestor: { id: string; title: string };
  blockers: InheritedBlocker[];
}

/**
 * The Task view action for a key press, or `null`: `Escape` returns to the
 * Work list and `]` collapses or expands the properties panel. Keys typed
 * into a form field or the editor, held with a modifier, or already handled
 * are left alone.
 */
export function taskKeyAction(event: WorkKey & { defaultPrevented: boolean }): TaskKeyAction | null {
  if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || inField(event.target)) return null;
  if (event.key === "Escape") return "back";
  if (event.key === "]") return "properties";
  return null;
}

/** The title of every record `detail` names, by ID. */
export function titlesOf(detail: TaskDetail): Map<string, string> {
  const tasks: Task[] = [
    detail.task,
    ...detail.ancestors,
    ...detail.children,
    ...detail.dependents,
    ...detail.blockers.map((blocker) => blocker.task),
    ...detail.inherited_blockers.map((blocker) => blocker.task),
  ];
  const containers = [detail.spec, detail.plan, detail.phase].filter((container) => container !== null);
  return new Map([...tasks, ...containers].map((record): [string, string] => [record.id, record.title]));
}

/**
 * Split a reason's message at the IDs it quotes. A task the reason names
 * becomes a link to its Task view, titled when `titles` knows it; a spec,
 * plan or phase becomes its title, since it has no view yet; any other
 * quoted text, such as a status, stays as written.
 */
export function reasonSegments(reason: ReadinessReason, titles: Map<string, string>): ReasonSegment[] {
  const tasks = new Set([reason.task, reason.ancestor, reason.blocker, ...(reason.path ?? [])].filter(Boolean));
  return reason.message
    .split(/`([^`]+)`/)
    .map((part, index): ReasonSegment => {
      if (index % 2 === 0) return { text: part };
      if (tasks.has(part)) return { id: part, label: titles.get(part) ?? part, href: taskHref(part) };
      return { text: part === reason.container ? (titles.get(part) ?? part) : part };
    })
    .filter((segment) => !("text" in segment) || segment.text !== "");
}

/** Group inherited blockers by the ancestor that holds them, the root first. */
export function inheritedGroups(detail: TaskDetail): InheritedGroup[] {
  const titles = titlesOf(detail);
  const holders = [...new Set([...detail.ancestors.map((ancestor) => ancestor.id), ...detail.inherited_blockers.map((b) => b.ancestor)])];
  return holders.flatMap((id) => {
    const blockers = detail.inherited_blockers.filter((blocker) => blocker.ancestor === id);
    return blockers.length ? [{ ancestor: { id, title: titles.get(id) ?? id }, blockers }] : [];
  });
}

/** The words for a task's readiness: ready, blocked, or not ready for another reason. */
export function readinessLabel(detail: TaskDetail): string {
  if (detail.readiness.ready) return "Ready";
  return detail.readiness.blocked ? "Blocked" : "Not ready";
}

/** The rows of the Task view's properties panel. */
export function propertiesOf(detail: TaskDetail): PropertyItem[] {
  const { task } = detail;
  const parent = detail.ancestors.at(-1) ?? null;
  return [
    { label: "Status", value: formatStatus(task.status), status: task.status },
    { label: "Readiness", value: readinessLabel(detail) },
    { label: "Priority", value: task.priority, priority: task.priority },
    task.parent_id === null
      ? { label: "Parent", value: null }
      : { label: "Parent", value: parent?.id === task.parent_id ? parent.title : task.parent_id, href: taskHref(task.parent_id) },
    { label: "Spec", value: detail.spec?.title ?? null },
    { label: "Plan", value: detail.plan?.title ?? null },
    { label: "Phase", value: detail.phase?.title ?? null },
  ];
}
