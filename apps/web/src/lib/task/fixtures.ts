import type { TaskDetail } from "$lib/data/types";
import { task } from "$lib/work/fixtures";

/**
 * T10.4 under T10, blocked directly by T10.3 and, through T10, by T17; it
 * carries a handoff and evidence and unblocks T10.5.
 */
export function blockedDetail(): TaskDetail {
  const parent = task("t-10", { title: "T10: Add the MCP server", phase_id: "ph-5" });
  return {
    task: task("t-10.4", {
      title: "T10.4: Run capture to completion through MCP",
      parent_id: "t-10",
      body: "Prove the MCP server end to end.\n\n- [ ] A run passes through MCP tools only",
      handoff: "Reads pass. Mutations still need the **authorization** check.",
      evidence: "Delivered by #41.",
    }),
    readiness: {
      ready: false,
      blocked: true,
      reasons: [
        { kind: "direct_blocker", message: "blocked by `t-10.3` (in_progress)", blocker: "t-10.3" },
        {
          kind: "inherited_blocker",
          message: "ancestor `t-10` is blocked by `t-17` (pending)",
          ancestor: "t-10",
          blocker: "t-17",
        },
        { kind: "closed_container", message: "phase `ph-5` is `completed`", container: "ph-5" },
      ],
    },
    blockers: [
      {
        task: task("t-10.3", { title: "T10.3: Add the MCP server", status: "in_progress" }),
        completed: false,
        evidence: "",
      },
    ],
    inherited_blockers: [
      {
        task: task("t-17", { title: "T17: Report readiness as data" }),
        completed: false,
        evidence: "",
        ancestor: "t-10",
      },
    ],
    dependents: [task("t-10.5", { title: "T10.5: Document the MCP tools" })],
    children: [],
    ancestors: [parent],
    spec: { id: "s-1", title: "First release", status: "open" },
    plan: { id: "pl-1", title: "Roadmap", status: "open" },
    phase: { id: "ph-5", plan_id: "pl-1", title: "Milestone 5: Agent interface", status: "completed", position: 5 },
  };
}

/**
 * {@link blockedDetail} with two completed blockers as `arcl show` lists
 * them: T10.2 directly, with evidence, and T9 through T10, without.
 */
export function doneBlockersDetail(): TaskDetail {
  const detail = blockedDetail();
  const done = (id: string, title: string) => task(id, { title, status: "completed" });
  detail.blockers.push({
    task: done("t-10.2", "T10.2: Define the tool schema"),
    completed: true,
    evidence: "Landed in **#38**.",
  });
  detail.inherited_blockers.push({
    task: done("t-9", "T9: Ship the store"),
    completed: true,
    evidence: "",
    ancestor: "t-10",
  });
  return detail;
}

/** A ready task with no relations, handoff or evidence. */
export function bareDetail(): TaskDetail {
  return {
    task: task("t-1", { title: "Add the skill" }),
    readiness: { ready: true, blocked: false, reasons: [] },
    blockers: [],
    inherited_blockers: [],
    dependents: [],
    children: [],
    ancestors: [],
    spec: null,
    plan: null,
    phase: null,
  };
}
