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
 * `REF` is a task ID, or the key a task title starts with, such as `T18.4`
 * for `T18.4: Add task mutations`. A word that only looks like a token, such
 * as `!later` or a bare `^`, stays in the title. The body is never scanned.
 */
export function parseCapture(text: string): ParsedCapture {
  const newline = text.indexOf("\n");
  const line = (newline === -1 ? text : text.slice(0, newline)).trim();
  const body = newline === -1 ? "" : text.slice(newline + 1).trim();
  const tokens: CaptureToken[] = [];
  const words = line.split(/\s+/).filter((word) => {
    const token = tokenOf(word);
    if (token) tokens.push(token);
    return word !== "" && !token;
  });
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
  for (const token of parsed.tokens) {
    if (token.kind === "priority") task.priority = token.priority;
    else if (token.kind === "parent") task.parent = resolve(token.ref);
    else {
      const id = resolve(token.ref);
      if (!blockers.includes(id)) blockers.push(id);
    }
  }
  if (blockers.length) task.blocked_by = blockers;
  return problems.length ? { problems } : { task };
}

/**
 * The ID of the one task `ref` names: a task whose ID it is, or else whose
 * title starts with it followed by `:` or a space, ignoring case.
 */
export function resolveRef(ref: string, tasks: TaskSummary[]): string | RefProblem {
  const byId = tasks.find((task) => task.id === ref);
  if (byId) return byId.id;
  const key = ref.toLowerCase();
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

function tokenOf(word: string): CaptureToken | null {
  if (word.startsWith("!")) {
    const priority = word.slice(1).toLowerCase();
    return (PRIORITIES as readonly string[]).includes(priority) ? { kind: "priority", text: word, priority: priority as TaskPriority } : null;
  }
  if (word.startsWith("^") && word.length > 1) return { kind: "parent", text: word, ref: word.slice(1) };
  if (word.toLowerCase().startsWith("after:") && word.length > 6) return { kind: "blocker", text: word, ref: word.slice(6) };
  return null;
}
