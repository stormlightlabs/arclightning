/**
 * The `data` of the `arcl serve` read envelopes, and how each becomes what a
 * view reads. The artifact database holds the same envelopes, so both
 * backends read them through these functions.
 */
import {
  DataError,
  type Capture,
  type CaptureStatus,
  type CaptureSummary,
  type Milestone,
  type Readiness,
  type Task,
  type TaskDetail,
  type TaskStatus,
  type TaskSummary,
  type TreeNode,
  type WorkItem,
  type WorkList,
} from "./types";

/** A record as `arcl list --json` lists it. */
export interface ListedRecord {
  id: string;
  kind: string;
  status: string;
  title: string;
}

/** The `data` of `arcl list --json`, and of `/api/records`. */
export interface Listing {
  records: ListedRecord[];
}

/** A phase as `arcl serve` returns it. */
export interface Phase {
  id: string;
  plan_id: string;
  title: string;
  status: Milestone["status"];
  position: number;
}

/** The `data` of `/api/work`: every task with its readiness and phase, and every phase. */
export interface Work {
  tasks: { task: Task; readiness: Readiness; phase_id: string | null }[];
  phases: Phase[];
}

/** The `data` of `arcl tree --json`, and of `/api/tree`. */
export interface Tree {
  nodes: TreeNode[];
}

/** The Work list from `/api/work`'s data. */
export function workOf(work: Work): WorkList {
  const milestones = work.phases.map(milestoneOf);
  const byId = new Map(milestones.map((milestone) => [milestone.id, milestone]));
  const items = work.tasks.map(({ task, readiness, phase_id }): WorkItem => ({
    task,
    readiness,
    milestone: phase_id === null ? null : (byId.get(phase_id) ?? null),
  }));
  return { milestones, items };
}

/** The Task view's detail from `arcl show <id>`'s data, which must be a task's. */
export function taskOf(id: string, detail: Partial<TaskDetail>): TaskDetail {
  // `arcl show` answers for any record kind; only a task has readiness.
  if (!("readiness" in detail)) throw new DataError("not_a_task", `\`${id}\` is not a task`);
  return detail as TaskDetail;
}

/** The capture from `arcl show <id>`'s data, which must be a capture's. */
export function captureOf(id: string, shown: { capture?: Capture }): Capture {
  if (shown.capture === undefined) throw new DataError("not_a_capture", `\`${id}\` is not a capture`);
  return shown.capture;
}

/** Each task's ID, title and status from `arcl list --kind task`'s data. */
export function taskIndexOf({ records }: Listing): TaskSummary[] {
  return records.map(({ id, title, status }) => ({ id, title, status: status as TaskStatus }));
}

/** Each capture's ID, title and status from `arcl list --kind capture`'s data. */
export function capturesOf({ records }: Listing): CaptureSummary[] {
  return records.map(({ id, title, status }) => ({ id, title, status: status as CaptureStatus }));
}

/** A milestone from a phase, without the fields the views do not read. */
export function milestoneOf({ id, plan_id, title, status, position }: Phase): Milestone {
  return { id, plan_id, title, status, position };
}
