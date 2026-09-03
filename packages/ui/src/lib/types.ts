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
