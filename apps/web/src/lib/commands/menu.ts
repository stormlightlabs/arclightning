import { formatStatus, type CommandGroup, type CommandItem, type IconName } from "@arclightning/ui";
import type { TaskSummary } from "$lib/data/types";
import { formatKeys } from "./keys";

/** How many matching tasks the menu lists. */
export const TASK_RESULTS = 8;

/**
 * Where a command applies, which orders the menu: the selected task's
 * actions first, then the open view's commands, then the commands every view
 * has.
 */
export type CommandScope = "selection" | "view" | "global";

/** An action the command menu lists and a shortcut can run. */
export interface Command {
  /** Unique among the commands registered at once. */
  id: string;
  label: string;
  scope: CommandScope;
  /** The group heading for `view` and `global` commands, such as `Go to`. */
  group?: string;
  /** Muted text after the label. */
  detail?: string;
  /** The keys that run it, pressed one after another, such as `["g", "w"]`. */
  keys?: string[];
  /**
   * Whether the view already answers `keys` itself, such as `j` in the Work
   * list. The menu shows them, and the shortcut registry leaves them alone.
   */
  viewKeys?: boolean;
  icon?: IconName;
  run: () => void;
}

/** The record the selection commands act on, shown as the menu's chip. */
export interface CommandContext {
  id: string;
  label: string;
}

/** What {@link menuGroups} builds the menu from. */
export interface MenuInput {
  commands: Command[];
  query: string;
  /** The selected record; `null` drops the selection commands. */
  context: CommandContext | null;
  /** Tasks to search; matches show only once the query has text. */
  tasks: TaskSummary[];
  mac: boolean;
}

/** The ID prefix of a task search result in the menu, before the task's ID. No command ID starts with it. */
export const TASK_RESULT_PREFIX = "search:";

/**
 * The command menu's groups for `input`: the actions on the selection first,
 * under `Actions on <label>`, then each view and global group in the order its
 * first command registered, then matching tasks under `Tasks`. A command or
 * task matches when its label holds every word of the query, in any case.
 */
export function menuGroups({ commands, query, context, tasks, mac }: MenuInput): CommandGroup[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const matches = (text: string) => words.every((word) => text.toLowerCase().includes(word));
  const item = (command: Command): CommandItem => ({
    id: command.id,
    label: command.label,
    detail: command.detail,
    keys: command.keys && formatKeys(command.keys, mac),
    icon: command.icon,
  });

  const groups: CommandGroup[] = [];
  const shown = commands.filter(
    (command) => matches(command.label) && (command.scope !== "selection" || context !== null),
  );
  if (context !== null) {
    const selection = shown.filter((command) => command.scope === "selection");
    if (selection.length)
      groups.push({ id: "selection", label: `Actions on ${context.label}`, items: selection.map(item) });
  }
  for (const scope of ["view", "global"] as const) {
    const byGroup = new Map<string, Command[]>();
    for (const command of shown.filter((command) => command.scope === scope)) {
      const name = command.group ?? (scope === "view" ? "This view" : "General");
      byGroup.set(name, [...(byGroup.get(name) ?? []), command]);
    }
    for (const [name, members] of byGroup)
      groups.push({ id: `${scope}:${name}`, label: name, items: members.map(item) });
  }
  if (words.length) {
    const found = tasks
      .filter((task) => matches(task.title) || task.id.toLowerCase() === query.trim().toLowerCase())
      .slice(0, TASK_RESULTS);
    if (found.length) {
      groups.push({
        id: "tasks",
        label: "Tasks",
        items: found.map((task) => ({
          id: `${TASK_RESULT_PREFIX}${task.id}`,
          label: task.title,
          detail: formatStatus(task.status),
          icon: "task",
        })),
      });
    }
  }
  return groups;
}

/**
 * The commands whose shortcuts the registry answers: those with keys that
 * the view does not answer itself, the selection's only while there is one.
 */
export function boundCommands(commands: Command[], context: CommandContext | null): (Command & { keys: string[] })[] {
  return commands.filter(
    (command): command is Command & { keys: string[] } =>
      command.keys !== undefined && !command.viewKeys && (command.scope !== "selection" || context !== null),
  );
}

/**
 * Shortcuts that two commands share, as `keys → labels`, so a test can keep
 * every view's shortcuts apart from the global ones.
 */
export function clashes(commands: Command[]): string[] {
  const seen = new Map<string, string>();
  const found: string[] = [];
  for (const command of commands) {
    if (!command.keys) continue;
    const keys = command.keys.join(" ");
    const other = seen.get(keys);
    if (other !== undefined && other !== command.label) found.push(`${keys} → ${other}, ${command.label}`);
    seen.set(keys, command.label);
  }
  return found;
}
