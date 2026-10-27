import {
  DataError,
  type ChangeWarning,
  type DataSource,
  type Task,
  type TaskDetail,
  type TaskEdit,
  type TaskPriority,
  type TaskStatus,
} from "$lib/data/types";

/** A lifecycle change the Task view offers. */
export type TaskAction = "start" | "complete" | "handoff" | "park" | "unpark";

/** The fields the Task view's edit form changes. */
export interface TaskDraft {
  title: string;
  body: string;
  priority: TaskPriority;
}

/** What a finished action reports to the view. */
export interface ActionResult {
  /** The tasks a completion made ready, by ID; empty for other actions. */
  unblocked: string[];
  /** The warnings the change's syncs gave; the change is saved. */
  warnings: ChangeWarning[];
}

/** A failed change, as the view shows it next to the action. */
export interface ActionFailure {
  /** The CLI's error code, such as `invalid_request`. */
  code: string;
  message: string;
}

/** The button label of each action. */
export const ACTION_LABELS: Record<TaskAction, string> = {
  start: "Start",
  complete: "Complete",
  handoff: "Hand off",
  park: "Park",
  unpark: "Unpark",
};

/**
 * The key that runs each action wherever a task is selected: in the Task
 * view, and on the selected row of the Work list.
 */
export const ACTION_KEYS: Record<TaskAction, string> = {
  start: "s",
  complete: "c",
  handoff: "h",
  park: "p",
  unpark: "u",
};

/** The key that opens the Task view's edit form. */
export const EDIT_KEY = "e";

/**
 * The actions that take Markdown before they run, with the label of the text
 * they take: completion evidence, which is optional, and a handoff note.
 */
export const ACTION_TEXT: Partial<Record<TaskAction, string>> = {
  complete: "Evidence",
  handoff: "Handoff note",
};

/**
 * The action `value` names when that action opens a form, such as the
 * `action` query parameter, else `null`. A URL only opens a form: it never
 * names an action that runs as soon as it is chosen.
 */
export function formAction(value: string | null): TaskAction | null {
  return value !== null && Object.hasOwn(ACTION_TEXT, value) ? (value as TaskAction) : null;
}

/**
 * The actions a task in `status` can take, the usual next step first. The
 * CLI still decides: a pending task that is not ready offers Start, and
 * the refusal names the reasons.
 */
export function actionsFor(status: TaskStatus): TaskAction[] {
  switch (status) {
    case "pending":
      return ["start", "park"];
    case "in_progress":
      return ["complete", "handoff", "park"];
    case "parked":
      return ["unpark"];
    default:
      return [];
  }
}

/**
 * Run `action` on task `id` through `source`. `text` is the evidence for a
 * completion, left out when blank, or the note for a handoff.
 */
export async function runAction(source: DataSource, id: string, action: TaskAction, text = ""): Promise<ActionResult> {
  switch (action) {
    case "start":
      return { unblocked: [], warnings: (await source.startTask(id)).warnings };
    case "complete": {
      const { value, warnings } = await source.completeTask(id, text.trim() === "" ? {} : { evidence: text });
      return { unblocked: value.unblocked, warnings };
    }
    case "handoff":
      return { unblocked: [], warnings: (await source.handOffTask(id, text)).warnings };
    case "park":
      return { unblocked: [], warnings: (await source.parkTask(id)).warnings };
    case "unpark":
      return { unblocked: [], warnings: (await source.unparkTask(id)).warnings };
  }
}

/** The edit form's starting values for `task`. */
export function draftOf(task: Task): TaskDraft {
  return { title: task.title, body: task.body, priority: task.priority };
}

/**
 * The fields of `draft` that differ from `task`, as an edit. The title goes
 * as typed, so the CLI's title rules apply to it unchanged.
 */
export function editOf(task: Task, draft: TaskDraft): TaskEdit {
  const edit: TaskEdit = {};
  if (draft.title !== task.title) edit.title = draft.title;
  if (draft.body !== task.body) edit.body = draft.body;
  if (draft.priority !== task.priority) edit.priority = draft.priority;
  return edit;
}

/** `error` as the view shows it: the CLI's code and message when it has them. */
export function failureOf(error: unknown): ActionFailure {
  if (error instanceof DataError) return { code: error.code, message: error.message };
  return { code: "unavailable", message: error instanceof Error ? error.message : String(error) };
}

/**
 * The tasks a completion unblocked, titled from the reloaded `detail`, whose
 * dependents include them; an ID it does not name stands as its own title.
 */
export function unblockedTasks(ids: string[], detail: TaskDetail): { id: string; title: string }[] {
  const titles = new Map(detail.dependents.map((task) => [task.id, task.title]));
  return ids.map((id) => ({ id, title: titles.get(id) ?? id }));
}
