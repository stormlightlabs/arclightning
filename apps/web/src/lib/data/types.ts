/**
 * Record shapes the web app reads, named and cased as in the CLI's `--json`
 * output so every backend can hand them over unchanged.
 */

/** A task's lifecycle state. */
export type TaskStatus = "pending" | "in_progress" | "parked" | "completed" | "cancelled";

/** A task's priority. */
export type TaskPriority = "critical" | "high" | "normal" | "low";

/** The lifecycle state of a container: a phase, plan or release. */
export type ContainerStatus = "open" | "completed" | "cancelled";

/** A task record, as `arcl show <task-id> --json` returns it. */
export interface Task {
  id: string;
  spec_id: string | null;
  plan_id: string | null;
  phase_id: string | null;
  parent_id: string | null;
  title: string;
  body: string;
  status: TaskStatus;
  priority: TaskPriority;
  position: number;
  handoff: string;
  evidence: string;
}

/** A milestone: a phase of a plan. */
export interface Milestone {
  id: string;
  /** The plan the phase belongs to. */
  plan_id: string;
  title: string;
  status: ContainerStatus;
  /** The phase's place in its plan, which orders milestones as `arcl tree` does. */
  position: number;
}

/** One reason a task is not ready, with the records it names. */
export interface ReadinessReason {
  /** A stable name for the reason, such as `direct_blocker`. */
  kind: string;
  message: string;
  task?: string;
  ancestor?: string;
  blocker?: string;
  container?: string;
  /** The task IDs that form a dependency cycle. */
  path?: string[];
}

/** Whether a task can start, as `arcl explain --json` reports it. */
export interface Readiness {
  ready: boolean;
  /** Whether a blocker on the task or an ancestor is incomplete. */
  blocked: boolean;
  reasons: ReadinessReason[];
}

/** A task with its readiness and the milestone it falls under. */
export interface WorkItem {
  task: Task;
  readiness: Readiness;
  /** The task's phase, or its nearest ancestor's; `null` outside any phase. */
  milestone: Milestone | null;
}

/**
 * What the Work view shows. Lists may come in any order: the Work view sorts
 * them as `arcl tree` does.
 */
export interface WorkList {
  milestones: Milestone[];
  items: WorkItem[];
}

/**
 * Where views read records from. `arcl serve` backs it locally; another
 * backend, such as a claude.ai artifact's database, can stand in for it.
 */
export interface DataSource {
  /** Every task, with its readiness and milestone. */
  work(): Promise<WorkList>;
}

/** A failed read, with the CLI's error code and message. */
export class DataError extends Error {
  /** The CLI's error code, such as `not_found`. */
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "DataError";
    this.code = code;
  }
}
