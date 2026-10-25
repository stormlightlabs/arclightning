import type { CapturePromotion, PromotionTarget, TaskPriority, TaskSummary } from "$lib/data";
import { parseCapture, resolveRef } from "$lib/capture";

/** What triage does with the selected capture. */
export type TriageAction = PromotionTarget | "discard";

/** The key that runs each triage action on the selected capture. */
export const TRIAGE_KEYS: Record<TriageAction, string> = {
  task: "1",
  spec: "2",
  note: "3",
  discard: "Backspace",
};

/** The button label of each triage action. */
export const TRIAGE_LABELS: Record<TriageAction, string> = {
  task: "Promote to task",
  spec: "Promote to spec",
  note: "Make a note",
  discard: "Discard",
};

/** The Promote to task form. `phase` and `parent` are IDs, or `""` for none. */
export interface TaskForm {
  to: "task";
  title: string;
  priority: TaskPriority;
  phase: string;
  parent: string;
  /** What the capture's tokens ask for that the form does not set, one line each. */
  notes: string[];
}

/** The Promote to spec form. */
export interface SpecForm {
  to: "spec";
  title: string;
  /** Markdown acceptance criteria; blank leaves them out. */
  acceptance: string;
}

/** The Make a note form. */
export interface NoteForm {
  to: "note";
  title: string;
}

/** The form a promotion fills in before it runs. */
export type PromotionForm = TaskForm | SpecForm | NoteForm;

/**
 * The form that promotes a capture titled `title` to `to`.
 *
 * A capture keeps quick capture's tokens in its title, so the title loses
 * them here. For a task, `!priority` sets the priority and the last `^REF`
 * the parent, looked up in `tasks`. A promotion takes no blockers, so each
 * `after:REF` becomes a note, as does a parent that names no task or several.
 */
export function formFor(to: PromotionTarget, title: string, tasks: TaskSummary[]): PromotionForm {
  const parsed = parseCapture(title);
  const name = parsed.title === "" ? title : parsed.title;
  if (to === "spec") return { to, title: name, acceptance: "" };
  if (to === "note") return { to, title: name };

  const form: TaskForm = { to, title: name, priority: "normal", phase: "", parent: "", notes: [] };
  const parent = parsed.tokens.findLast((token) => token.kind === "parent");
  for (const token of parsed.tokens) {
    if (token.kind === "priority") form.priority = token.priority;
    else if (token.kind === "blocker") form.notes.push(`\`${token.text}\` is not set: add the blocker after promoting, with arcl dependency add.`);
    else if (token === parent) {
      const found = resolveRef(token.ref, tasks);
      if (typeof found === "string") form.parent = found;
      else form.notes.push(`\`${token.text}\` ${found.problem === "missing" ? "names no task" : "names more than one task"}, so no parent is set.`);
    }
  }
  return form;
}

/** The options `arcl capture promote` takes for `form`, leaving out what is blank. */
export function promotionOf(form: PromotionForm): CapturePromotion {
  switch (form.to) {
    case "task": {
      const promotion: CapturePromotion = { to: "task", title: form.title, priority: form.priority };
      if (form.phase !== "") promotion.phase = form.phase;
      if (form.parent !== "") promotion.parent = form.parent;
      return promotion;
    }
    case "spec":
      return form.acceptance.trim() === "" ? { to: "spec", title: form.title } : { to: "spec", title: form.title, acceptance_criteria: form.acceptance };
    case "note":
      return { to: "note", title: form.title };
  }
}

/**
 * The row to select once `removed` leaves `order`: the one after it, else the
 * one before, else none.
 */
export function afterRemoval(order: string[], removed: string): string | null {
  const index = order.indexOf(removed);
  if (index === -1) return order[0] ?? null;
  return order[index + 1] ?? order[index - 1] ?? null;
}
