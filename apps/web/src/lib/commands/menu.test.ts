import { describe, expect, it } from "vitest";
import { TRIAGE_KEYS } from "$lib/inbox/triage";
import { PLANNING_KEYS } from "$lib/planning/view";
import { ACTION_KEYS, EDIT_KEY } from "$lib/task/actions";
import { TASK_RESULT_PREFIX, boundCommands, clashes, menuGroups, type Command, type MenuInput } from "./menu";

const run = () => {};
const commands: Command[] = [
  { id: "global:capture", label: "Quick capture", scope: "global", group: "Create", keys: ["q"], run },
  { id: "global:go-w", label: "Work", scope: "global", group: "Go to", keys: ["g", "w"], run },
  { id: "work:new", label: "New task", scope: "view", group: "Work", keys: ["n"], run },
  { id: "work:next", label: "Select the next task", scope: "view", group: "Work", keys: ["j"], viewKeys: true, run },
  { id: "work:open", label: "Open task", scope: "selection", keys: ["Enter"], viewKeys: true, run },
  { id: "work:start", label: "Start", scope: "selection", keys: ["s"], run },
  { id: "work:park", label: "Park", scope: "selection", keys: ["p"], run },
];
const tasks = [
  { id: "t-1", title: "T1: Start the server", status: "completed" as const },
  { id: "t-2", title: "T2: Add capture", status: "pending" as const },
];
const input = (fields: Partial<MenuInput> = {}): MenuInput => ({
  commands,
  query: "",
  context: { id: "t-2", label: "T2: Add capture" },
  tasks,
  mac: false,
  ...fields,
});

describe("menuGroups", () => {
  it("lists the actions on the selection first, each with its shortcut", () => {
    const groups = menuGroups(input());
    expect(groups.map((group) => group.label)).toEqual(["Actions on T2: Add capture", "Work", "Create", "Go to"]);
    expect(groups[0].items.map((item) => [item.label, item.keys])).toEqual([
      ["Open task", ["↵"]],
      ["Start", ["S"]],
      ["Park", ["P"]],
    ]);
    expect(groups[3].items[0].keys).toEqual(["G", "W"]);
  });

  it("drops the selection's actions when nothing is selected", () => {
    const groups = menuGroups(input({ context: null }));
    expect(groups.map((group) => group.label)).toEqual(["Work", "Create", "Go to"]);
  });

  it("filters by every word of the query and adds matching tasks", () => {
    const groups = menuGroups(input({ query: "st" }));
    expect(groups.map((group) => group.label)).toEqual(["Actions on T2: Add capture", "Tasks"]);
    expect(groups[0].items.map((item) => item.label)).toEqual(["Start"]);
    expect(groups[1].items).toEqual([
      { id: `${TASK_RESULT_PREFIX}t-1`, label: "T1: Start the server", detail: "Completed", icon: "task" },
    ]);
    expect(
      menuGroups(input({ query: "add CAPTURE" }))
        .at(-1)
        ?.items.map((item) => item.label),
    ).toEqual(["T2: Add capture"]);
    expect(
      menuGroups(input({ query: "t-2" }))
        .at(-1)
        ?.items.map((item) => item.id),
    ).toEqual([`${TASK_RESULT_PREFIX}t-2`]);
  });

  it("lists no tasks until the query has text", () => {
    expect(menuGroups(input()).some((group) => group.id === "tasks")).toBe(false);
    expect(menuGroups(input({ query: "zzz" }))).toEqual([]);
  });

  it("names ⌘ on macOS", () => {
    expect(
      menuGroups(input({ mac: true, commands: [{ id: "x", label: "X", scope: "global", keys: ["Mod", "k"], run }] }))[0]
        .items[0].keys,
    ).toEqual(["⌘", "K"]);
  });
});

describe("boundCommands", () => {
  it("binds shortcuts the view does not answer itself, the selection's only with a selection", () => {
    const ids = (context: MenuInput["context"]) => boundCommands(commands, context).map((command) => command.id);
    expect(ids({ id: "t-2", label: "T2" })).toEqual([
      "global:capture",
      "global:go-w",
      "work:new",
      "work:start",
      "work:park",
    ]);
    expect(ids(null)).toEqual(["global:capture", "global:go-w", "work:new"]);
  });
});

describe("shortcuts", () => {
  it("finds two commands that share keys", () => {
    expect(clashes(commands)).toEqual([]);
    expect(clashes([...commands, { id: "x", label: "Something", scope: "view", keys: ["q"], run }])).toEqual([
      "q → Quick capture, Something",
    ]);
  });

  it("gives each task action its own key, apart from the app's other keys", () => {
    const keys = [...Object.values(ACTION_KEYS), EDIT_KEY];
    expect(new Set(keys).size).toBe(keys.length);
    // Quick capture, Go to, New task, the Work list's moves and the Task view's panel.
    for (const taken of ["q", "g", "n", "j", "k", "]"]) expect(keys).not.toContain(taken);
  });

  it("keeps the Inbox's and Planning's keys apart from the app's and the list moves", () => {
    for (const keys of [Object.values(TRIAGE_KEYS), Object.values(PLANNING_KEYS)]) {
      expect(new Set(keys).size).toBe(keys.length);
      // Quick capture, Go to, and the moves and Enter every list answers.
      for (const taken of ["q", "g", "j", "k", "Enter", "ArrowUp", "ArrowDown"]) expect(keys).not.toContain(taken);
    }
  });
});
