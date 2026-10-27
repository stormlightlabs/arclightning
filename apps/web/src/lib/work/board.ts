import { formatStatus, type TaskGlyphKind } from "@arclightning/ui";
import type { TaskStatus, WorkItem, WorkList } from "$lib/data";
import type { TaskAction } from "$lib/task";
import { matches, shownReasons, type WorkFilters } from "./filters";
import { describeReason, glyphOf, orderMilestones, orderTasks } from "./grouping";

/** The Work page's layouts: a list grouped by milestone, or a board of status columns. */
export type WorkView = "list" | "board";

/** The Work page query parameter that names the layout. */
export const VIEW = "view";

/** The local storage key that remembers the viewer's last layout. */
export const VIEW_STORAGE_KEY = "arcl-work-view";

/** The board's columns, one per task status, in workflow order. */
export const BOARD_STATUSES: readonly TaskStatus[] = ["pending", "in_progress", "parked", "completed", "cancelled"];

/**
 * The action that moves a task from one status to another, for each move the
 * web app offers: `start`, `park`, `unpark` and `complete`. The web app has
 * no cancel action, and no command reopens a closed task, so those moves have
 * no action.
 */
const MOVES: Partial<Record<TaskStatus, Partial<Record<TaskStatus, TaskAction>>>> = {
  pending: { in_progress: "start", parked: "park", completed: "complete" },
  in_progress: { parked: "park", completed: "complete" },
  parked: { pending: "unpark" },
};

/** One card on the board. */
export interface BoardCard {
  id: string;
  title: string;
  status: TaskStatus;
  priority: WorkItem["task"]["priority"];
  /** The glyph the list shows for the task: its status once started or closed, else its readiness. */
  glyph: TaskGlyphKind;
  /** Why the task is not ready, with record IDs replaced by titles. */
  reasons: string[];
}

/** One status column and its cards, in `arcl tree` order. */
export interface BoardColumn {
  status: TaskStatus;
  title: string;
  cards: BoardCard[];
}

/**
 * The board for `list`: a column for each of {@link BOARD_STATUSES}, holding
 * the tasks that pass `filters`. Every column shows, empty or not, so a card
 * always has somewhere to go.
 */
export function boardColumns(list: WorkList, filters: WorkFilters): BoardColumn[] {
  const titles = new Map(list.items.map((item): [string, string] => [item.task.id, item.task.title]));
  const items = orderTasks(list.items, orderMilestones(list)).filter((item) => matches(item, filters));
  return BOARD_STATUSES.map((status) => ({
    status,
    title: formatStatus(status),
    cards: items
      .filter((item) => item.task.status === status)
      .map((item) => ({
        id: item.task.id,
        title: item.task.title,
        status,
        priority: item.task.priority,
        glyph: glyphOf(item),
        reasons: shownReasons(item.task, item.readiness).map((reason) => describeReason(reason, titles)),
      })),
  }));
}

/** The cards' IDs in board order: column by column, top to bottom. */
export function boardOrder(columns: BoardColumn[]): string[] {
  return columns.flatMap((column) => column.cards.map((card) => card.id));
}

/** The action that moves a task from `from` to `to`, or `null` when the web app has no action for that move. */
export function moveAction(from: TaskStatus, to: TaskStatus): TaskAction | null {
  return MOVES[from]?.[to] ?? null;
}

/** The statuses a task in `status` can move to, in column order. */
export function moveTargets(status: TaskStatus): TaskStatus[] {
  return BOARD_STATUSES.filter((target) => moveAction(status, target) !== null);
}

/**
 * Why a task in `from` cannot move to `to`, for the message a refused move
 * shows. `arcl task cancel` cancels an open task, but the web app has no
 * cancel action, so a move to Cancelled names the command.
 */
export function refusal(from: TaskStatus, to: TaskStatus): string {
  const targets = moveTargets(from).map(formatStatus);
  const allowed = targets.length === 0 ? "A task there does not move." : `It can move to ${targets.join(" or ")}.`;
  const cancellable = to === "cancelled" && from !== "completed" && from !== "cancelled";
  const reason = cancellable
    ? "The web app has no cancel action; run `arcl task cancel` to cancel a task."
    : `No task command moves a task from ${formatStatus(from)} to ${formatStatus(to)}.`;
  return `${reason} ${allowed}`;
}

/**
 * The layout a Work page URL asks for, or else the one `storage` remembers,
 * or else the list. `storage` is a function because reading
 * `window.localStorage` can throw, as on a sandboxed artifact page.
 */
export function viewOf(search: URLSearchParams, storage: () => Pick<Storage, "getItem">): WorkView {
  const asked = search.get(VIEW);
  if (asked === "board" || asked === "list") return asked;
  try {
    return storage().getItem(VIEW_STORAGE_KEY) === "board" ? "board" : "list";
  } catch {
    return "list";
  }
}

/** Remember `view` as the viewer's layout, when `storage` allows it. */
export function rememberView(storage: () => Pick<Storage, "setItem">, view: WorkView): void {
  try {
    storage().setItem(VIEW_STORAGE_KEY, view);
  } catch {
    // Without storage the layout lasts while the URL names it.
  }
}

/** A copy of `search` that names `view`. The list is the default, so it leaves the parameter out. */
export function writeView(view: WorkView, search: URLSearchParams): URLSearchParams {
  const next = new URLSearchParams(search);
  if (view === "board") next.set(VIEW, "board");
  else next.delete(VIEW);
  return next;
}
