import { describe, expect, it } from "vitest";
import { BOARD_STATUSES, VIEW_STORAGE_KEY, boardColumns, boardOrder, moveAction, moveTargets, refusal, rememberView, viewOf, writeView } from "./board";
import { sampleList } from "./fixtures";
import type { WorkFilters } from "./filters";

const none: WorkFilters = { status: [], readiness: [], priority: [], milestone: [] };

/** A stand-in for local storage. */
function memory(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial));
  return { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => void values.set(key, value), values };
}

describe("boardColumns", () => {
  it("shows a column per status in workflow order, with cards in tree order", () => {
    const columns = boardColumns(sampleList(), none);
    expect(columns.map((column) => [column.title, column.cards.map((card) => card.id)])).toEqual([
      ["Pending", ["t-3", "t-8", "t-7", "t-2", "t-5"]],
      ["In progress", ["t-4"]],
      ["Parked", ["t-6"]],
      ["Completed", ["t-1"]],
      ["Cancelled", []],
    ]);
    expect(boardOrder(columns)).toEqual(["t-3", "t-8", "t-7", "t-2", "t-5", "t-4", "t-6", "t-1"]);
  });

  it("puts readiness, priority and the reasons with titles on each card", () => {
    const pending = boardColumns(sampleList(), none)[0].cards;
    expect(pending.find((card) => card.id === "t-3")).toMatchObject({ readiness: "blocked", glyph: "blocked", reasons: ["blocked by Task t-2 (pending)"] });
    expect(pending.find((card) => card.id === "t-2")).toMatchObject({ readiness: "ready", glyph: "ready", priority: "high" });
  });

  it("follows the filters and keeps every column", () => {
    const columns = boardColumns(sampleList(), { ...none, readiness: ["ready"], milestone: ["ph-5"] });
    expect(columns.map((column) => column.cards.map((card) => card.id))).toEqual([["t-8", "t-7", "t-2"], [], [], [], []]);
    expect(columns.map((column) => column.status)).toEqual([...BOARD_STATUSES]);
  });
});

describe("moves", () => {
  it("allows only the moves a task command makes", () => {
    expect(moveAction("pending", "in_progress")).toBe("start");
    expect(moveAction("pending", "parked")).toBe("park");
    expect(moveAction("pending", "completed")).toBe("complete");
    expect(moveAction("in_progress", "parked")).toBe("park");
    expect(moveAction("in_progress", "completed")).toBe("complete");
    expect(moveAction("parked", "pending")).toBe("unpark");
    for (const [from, to] of [
      ["in_progress", "pending"],
      ["parked", "in_progress"],
      ["parked", "completed"],
      ["completed", "pending"],
      ["pending", "cancelled"],
      ["cancelled", "pending"],
    ] as const) {
      expect(moveAction(from, to), `${from} to ${to}`).toBeNull();
    }
    expect(moveTargets("pending")).toEqual(["in_progress", "parked", "completed"]);
    expect(moveTargets("completed")).toEqual([]);
  });

  it("says why a move is refused and where the task can go", () => {
    expect(refusal("parked", "completed")).toBe("No task command moves a task from Parked to Completed. It can move to Pending.");
    expect(refusal("completed", "pending")).toBe("No task command moves a task from Completed to Pending. A task there does not move.");
    expect(refusal("pending", "cancelled")).toBe(
      "The web app has no cancel action; run `arcl task cancel` to cancel a task. It can move to In progress or Parked or Completed.",
    );
    expect(refusal("completed", "cancelled")).toBe("No task command moves a task from Completed to Cancelled. A task there does not move.");
  });
});

describe("view", () => {
  it("reads the URL first, then the remembered layout, then the list", () => {
    const stored = memory({ [VIEW_STORAGE_KEY]: "board" });
    expect(viewOf(new URLSearchParams("view=list"), () => stored)).toBe("list");
    expect(viewOf(new URLSearchParams(""), () => stored)).toBe("board");
    expect(viewOf(new URLSearchParams("view=other"), () => memory())).toBe("list");
    expect(
      viewOf(new URLSearchParams(""), () => {
        throw new Error("no storage");
      }),
    ).toBe("list");
  });

  it("remembers the layout and writes it into the URL", () => {
    const stored = memory();
    rememberView(() => stored, "board");
    expect(stored.values.get(VIEW_STORAGE_KEY)).toBe("board");
    expect(writeView("board", new URLSearchParams("status=pending")).toString()).toBe("status=pending&view=board");
    expect(writeView("list", new URLSearchParams("view=board&status=pending")).toString()).toBe("status=pending");
  });
});
