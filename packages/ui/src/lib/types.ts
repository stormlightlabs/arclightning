import type { Priority } from "./components/PriorityBadge.svelte";
import type { RecordStatus } from "./components/StatusBadge.svelte";

/** Record kinds understood by shared planning presentation. */
export type PlanningRecordKind = "capture" | "spec" | "plan" | "phase" | "task" | "note";

/** Application-owned record data rendered by shared planning components. */
export interface PlanningRecordSummary {
  id: string;
  kind: PlanningRecordKind;
  title: string;
  description?: string;
  status?: RecordStatus;
  priority?: Priority;
  metadata?: string;
}

/** One node in a planning hierarchy supplied by an application. */
export interface PlanningTreeNode extends PlanningRecordSummary {
  children?: PlanningTreeNode[];
}

/**
 * The state a task's glyph shows: its status once work has started or ended,
 * and its readiness while it is pending or parked.
 */
export type TaskGlyphKind = "ready" | "blocked" | "waiting" | "in_progress" | "done" | "cancelled";

/** One task row in a task list, already shaped by the application. */
export interface TaskRowItem {
  id: string;
  title: string;
  status: RecordStatus;
  priority: Priority;
  glyph: TaskGlyphKind;
  /** Whether the task can be started now. */
  ready: boolean;
  /** Why the task is not ready, one line per reason. */
  reasons: string[];
}

/** A top-level group in a task list, such as a milestone. */
export interface TaskGroup {
  id: string;
  title: string;
  /** Text after the title, such as a count. */
  detail?: string;
  sections: TaskSection[];
}

/** A run of task rows inside a group, such as one status. */
export interface TaskSection {
  id: string;
  title: string;
  rows: TaskRowItem[];
}
