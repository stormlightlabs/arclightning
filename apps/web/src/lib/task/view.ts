import { formatStatus, type PropertyItem } from "@arclightning/ui";
import type { Blocker, GateKind, InheritedBlocker, ReadinessReason, Task, TaskDetail } from "$lib/data/types";
import { inField, type WorkKey } from "$lib/work/keys";
import { isClosed } from "$lib/work/filters";
import { taskHref } from "$lib/work/links";

/** What a key does in the Task view. */
export type TaskKeyAction = "back" | "properties";

/** A run of a readiness reason's text: plain, or a record it names. */
export type ReasonSegment = { text: string } | { id: string; label: string; href: string };

/** The unfinished inherited blockers one ancestor holds. */
export type InheritedGroup = {
  ancestor: { id: string; title: string };
  blockers: InheritedBlocker[];
};

/** A completed blocker, with the ancestor that holds it when it is inherited. */
export type DoneBlocker = {
  blocker: Blocker;
  ancestor: { id: string; title: string } | null;
};

/** A task's blockers: those it still waits on, and those already completed. */
export type Blockers = {
  /** Unfinished direct blockers. */
  direct: Blocker[];
  /** Unfinished inherited blockers, by the ancestor that holds them. */
  inherited: InheritedGroup[];
  /** Completed blockers, direct ones first. */
  done: DoneBlocker[];
};

/** The parts of a keyboard event {@link taskKeyAction} reads. */
export type TaskKey = WorkKey & {
  defaultPrevented: boolean;
  /** Absent where the event has no modifier state, as in tests. */
  getModifierState?: (key: string) => boolean;
};

/**
 * The Task view action for a key press, or `null`: `Escape` returns to the
 * Work list and `]` collapses or expands the properties panel. Keys typed
 * into a form field or the editor, or already handled, are left alone, and so
 * is `Escape` held with a modifier. `]` is taken whatever modifier a layout
 * needs to type it, AltGr (reported as Ctrl and Alt where the browser lacks
 * the `AltGraph` state) or Option, but not with Meta or with Ctrl alone.
 * `Escape` does nothing while `unsaved` says an open form holds input that
 * leaving would lose.
 */
export function taskKeyAction(event: TaskKey, unsaved = false): TaskKeyAction | null {
  if (event.defaultPrevented || event.metaKey || inField(event.target)) return null;
  if (event.key === "Escape") return event.altKey || event.ctrlKey || unsaved ? null : "back";
  if (event.key === "]") {
    const altGraph = event.getModifierState?.("AltGraph") ?? (event.ctrlKey && event.altKey);
    return event.ctrlKey && !altGraph ? null : "properties";
  }
  return null;
}

/**
 * The name of the copy button beside a related task. The ID tells apart tasks
 * that share a title, and `where` tells apart one task listed in two sections.
 */
export function copyLabel(task: { id: string; title: string }, where: string): string {
  return `Copy the ID of ${task.title} (${task.id}), ${where}`;
}

/**
 * A note's timestamp as `YYYY-MM-DD HH:MM UTC`, the form the Task view shows.
 * An RFC 3339 timestamp with any offset is shifted to UTC; a timestamp in
 * another form is shown as it is.
 */
export function noteTime(createdAt: string): string {
  const match = /^(\d{4}-\d{2}-\d{2})[Tt](\d{2}:\d{2}:\d{2})(?:\.(\d+))?([Zz]|[+-]\d{2}:\d{2})$/.exec(createdAt);
  if (match === null) {
    return createdAt;
  }
  const [, date, time, fraction = "", offset] = match;
  const millis = fraction.slice(0, 3).padEnd(3, "0");
  const parsed = new Date(`${date}T${time}.${millis}${offset.toUpperCase()}`);
  const iso = Number.isNaN(parsed.getTime()) ? "" : parsed.toISOString();
  if (!/^\d{4}-/.test(iso)) {
    return createdAt;
  }
  return `${iso.slice(0, 10)} ${iso.slice(11, 16)} UTC`;
}

/** The label the Task view gives a gate's kind. */
export function gateKindLabel(kind: GateKind): string {
  return kind === "pr" ? "Pull request" : kind === "run" ? "CI run" : "Date";
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

/** Group unfinished inherited blockers by the ancestor that holds them, the root first. */
export function inheritedGroups(detail: TaskDetail): InheritedGroup[] {
  const titles = titlesOf(detail);
  const waiting = detail.inherited_blockers.filter((blocker) => !blocker.completed);
  const holders = [...new Set([...detail.ancestors.map((ancestor) => ancestor.id), ...waiting.map((b) => b.ancestor)])];
  return holders.flatMap((id) => {
    const blockers = waiting.filter((blocker) => blocker.ancestor === id);
    return blockers.length ? [{ ancestor: { id, title: titles.get(id) ?? id }, blockers }] : [];
  });
}

/**
 * Split a task's blockers into those it still waits on and those completed.
 * `arcl show` lists both, and only the unfinished ones block.
 */
export function blockersOf(detail: TaskDetail): Blockers {
  const titles = titlesOf(detail);
  const done: DoneBlocker[] = [
    ...detail.blockers.filter((blocker) => blocker.completed).map((blocker) => ({ blocker, ancestor: null })),
    ...detail.inherited_blockers
      .filter((blocker) => blocker.completed)
      .map((blocker) => ({
        blocker,
        ancestor: { id: blocker.ancestor, title: titles.get(blocker.ancestor) ?? blocker.ancestor },
      })),
  ];
  return { direct: detail.blockers.filter((blocker) => !blocker.completed), inherited: inheritedGroups(detail), done };
}

/**
 * Whether the Task view shows the task's readiness. Readiness asks whether a
 * task can start, so a completed or cancelled task shows none.
 */
export function showsReadiness(detail: TaskDetail): boolean {
  return !isClosed(detail.task.status);
}

/**
 * The words for a task's readiness: ready, blocked, or not ready for another
 * reason. `null` for a closed task, whose Readiness property then shows
 * "None", as an empty Parent does.
 */
export function readinessLabel(detail: TaskDetail): string | null {
  if (!showsReadiness(detail)) return null;
  if (detail.readiness.ready) return "Ready";
  return detail.readiness.blocked ? "Blocked" : "Not ready";
}

/** The rows of the Task view's properties panel. */
export function propertiesOf(detail: TaskDetail): PropertyItem[] {
  const { task } = detail;
  const parent = detail.ancestors.at(-1) ?? null;
  return [
    { label: "ID", value: task.id, copy: true, copyLabel: "Copy this task's ID" },
    { label: "Status", value: formatStatus(task.status), status: task.status },
    { label: "Readiness", value: readinessLabel(detail) },
    { label: "Priority", value: task.priority, priority: task.priority },
    task.parent_id === null
      ? { label: "Parent", value: null }
      : {
          label: "Parent",
          value: parent?.id === task.parent_id ? parent.title : task.parent_id,
          href: taskHref(task.parent_id),
        },
    { label: "Spec", value: detail.spec?.title ?? null },
    { label: "Plan", value: detail.plan?.title ?? null },
    { label: "Phase", value: detail.phase?.title ?? null },
  ];
}
