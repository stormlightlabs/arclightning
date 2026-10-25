import type { NewCapture, NewTask, TaskPriority, TaskSummary } from "$lib/data";
import { PRIORITIES } from "$lib/work";

/** A token in the first line of quick capture text, and the field it sets. */
export type CaptureToken =
  | { kind: "priority"; text: string; priority: TaskPriority }
  | { kind: "parent"; text: string; ref: string }
  | { kind: "blocker"; text: string; ref: string };

/** Quick capture text split into its fields. */
export interface ParsedCapture {
  /** The first line without its tokens. */
  title: string;
  /** The lines after the first, as Markdown. */
  body: string;
  /** The first line as typed, tokens included. */
  line: string;
  tokens: CaptureToken[];
}

/** A task reference the task list does not resolve to one task. */
export interface RefProblem {
  ref: string;
  /** `missing` names no task; `ambiguous` names more than one. */
  problem: "missing" | "ambiguous";
}

/**
 * Split quick capture text into fields.
 *
 * The first line is the title and the lines after it are the body. In the
 * first line, a word that is one of these tokens sets a task field instead:
 *
 * - `!critical`, `!high`, `!normal` or `!low` sets the priority; the last wins
 * - `^REF` sets the parent task; the last wins
 * - `after:REF` adds a blocker, once for each token
 *
 * `REF` is a task ID such as `arcl-t-01M5…`, or a task number such as
 * `T18.4`, the key a title like `T18.4: Add task mutations` starts with.
 * A word that only looks like a token, such as `!later`, `^C` or
 * `after:lunch`, stays in the title. A leading backslash keeps a token in the
 * title without it: `\!high` is the word `!high`. The body is never scanned.
 */
export function parseCapture(text: string): ParsedCapture {
  const newline = text.indexOf("\n");
  const line = (newline === -1 ? text : text.slice(0, newline)).trim();
  const body = newline === -1 ? "" : text.slice(newline + 1).trim();
  const tokens: CaptureToken[] = [];
  const words: string[] = [];
  for (const word of line.split(/\s+/)) {
    if (word === "") continue;
    if (word.startsWith("\\") && tokenOf(word.slice(1))) {
      words.push(word.slice(1));
      continue;
    }
    const token = tokenOf(word);
    if (token) tokens.push(token);
    else words.push(word);
  }
  return { title: words.join(" "), body, line, tokens };
}

/** The capture `parsed` files: the first line as typed, tokens included, and the body. */
export function captureOf(parsed: ParsedCapture): NewCapture {
  return parsed.body === "" ? { title: parsed.line } : { title: parsed.line, body: parsed.body };
}

/**
 * The task `parsed` files, with each reference resolved through `tasks`, or
 * the references that resolve to no task or to several.
 */
export function taskOf(parsed: ParsedCapture, tasks: TaskSummary[]): { task: NewTask } | { problems: RefProblem[] } {
  const problems: RefProblem[] = [];
  const resolve = (ref: string): string => {
    const found = resolveRef(ref, tasks);
    if (typeof found === "string") return found;
    problems.push({ ref, problem: found.problem });
    return ref;
  };
  const task: NewTask = { title: parsed.title };
  if (parsed.body !== "") task.body = parsed.body;
  const blockers: string[] = [];
  // Only the last parent counts, so an earlier one is never resolved.
  const parent = parsed.tokens.findLast((token) => token.kind === "parent");
  for (const token of parsed.tokens) {
    if (token.kind === "priority") task.priority = token.priority;
    else if (token.kind === "parent") {
      if (token === parent) task.parent = resolve(token.ref);
    } else {
      const id = resolve(token.ref);
      if (!blockers.includes(id)) blockers.push(id);
    }
  }
  if (blockers.length) task.blocked_by = blockers;
  return problems.length ? { problems } : { task };
}

/**
 * The ID of the one task `ref` names: a task whose ID it is, or else whose
 * title starts with it followed by `:` or a space, ignoring case. IDs match
 * in any case too: the store writes each one in a single canonical form, a
 * lowercase prefix and an uppercase ULID, so no two differ only by case.
 */
export function resolveRef(ref: string, tasks: TaskSummary[]): string | RefProblem {
  const key = ref.toLowerCase();
  const byId = tasks.find((task) => task.id.toLowerCase() === key);
  if (byId) return byId.id;
  const byKey = tasks.filter((task) => {
    const title = task.title.toLowerCase();
    return title.startsWith(`${key}:`) || title.startsWith(`${key} `);
  });
  if (byKey.length === 1) return byKey[0].id;
  return { ref, problem: byKey.length === 0 ? "missing" : "ambiguous" };
}

/** What `token` sets, in words, such as `blocked by T18.4`. */
export function describeToken(token: CaptureToken): string {
  switch (token.kind) {
    case "priority":
      return `${token.priority} priority`;
    case "parent":
      return `subtask of ${token.ref}`;
    case "blocker":
      return `blocked by ${token.ref}`;
  }
}

/**
 * Whether `ref` has the shape of a task reference: a task ID such as
 * `arcl-t-01M5…`, or a task number such as `T18` or `T18.4`, in any case.
 * {@link resolveRef} still decides which task, if any, it names.
 */
export function isTaskRef(ref: string): boolean {
  return /^arcl-t-[0-9a-z]+$/i.test(ref) || /^t\d+(\.\d+)*$/i.test(ref);
}

function tokenOf(word: string): CaptureToken | null {
  if (word.startsWith("!")) {
    const priority = word.slice(1).toLowerCase();
    return (PRIORITIES as readonly string[]).includes(priority) ? { kind: "priority", text: word, priority: priority as TaskPriority } : null;
  }
  if (word.startsWith("^") && isTaskRef(word.slice(1))) return { kind: "parent", text: word, ref: word.slice(1) };
  if (word.toLowerCase().startsWith("after:") && isTaskRef(word.slice(6))) return { kind: "blocker", text: word, ref: word.slice(6) };
  return null;
}
