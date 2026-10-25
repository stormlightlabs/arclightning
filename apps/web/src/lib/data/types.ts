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

/** A spec or plan a task falls under, directly or through an ancestor. */
export interface Container {
  id: string;
  title: string;
  status: string;
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

/** A task that must complete before another can start. */
export interface Blocker {
  task: Task;
  completed: boolean;
  /** The evidence recorded when the blocker completed. */
  evidence: string;
}

/** A blocker held by an ancestor, which every descendant waits on too. */
export interface InheritedBlocker extends Blocker {
  /** The ancestor task that holds the blocker. */
  ancestor: string;
}

/** What the Task view shows, as `arcl show <task-id> --json` returns it. */
export interface TaskDetail {
  task: Task;
  readiness: Readiness;
  blockers: Blocker[];
  inherited_blockers: InheritedBlocker[];
  /** The tasks this task blocks. */
  dependents: Task[];
  children: Task[];
  /** The task's ancestors, the root first. */
  ancestors: Task[];
  /** The task's effective spec, plan and phase, its own or its nearest ancestor's. */
  spec: Container | null;
  plan: Container | null;
  phase: Milestone | null;
}

/** The options of `arcl task create`. `priority` defaults to `normal`. */
export interface NewTask {
  title: string;
  body?: string;
  priority?: TaskPriority;
  /** The parent task's ID. */
  parent?: string;
  spec?: string;
  plan?: string;
  phase?: string;
  /** The IDs of the tasks that must complete first. */
  blocked_by?: string[];
}

/** The options of `arcl task update`. A field left out keeps its value. */
export interface TaskEdit {
  title?: string;
  body?: string;
  priority?: TaskPriority;
}

/** The options of `arcl task complete`. */
export interface CompleteOptions {
  /** Markdown completion evidence. */
  evidence?: string;
  /** Complete only this task even when subtasks are open. */
  allow_open_children?: boolean;
  /** Also complete every pending and in-progress subtask. */
  with_subtasks?: boolean;
}

/** What `arcl task complete --json` returns. */
export interface TaskCompletion {
  task: Task;
  /** The subtasks completed with the task, by ID. */
  completed_subtasks: string[];
  /** The tasks the completion made ready, by ID. */
  unblocked: string[];
}

/** A task as `arcl list --json` names it, for search and references. */
export interface TaskSummary {
  id: string;
  title: string;
  status: TaskStatus;
}

/** A capture's state in the inbox. */
export type CaptureStatus = "captured" | "promoted" | "discarded";

/** A capture record, as `arcl capture create --json` returns it. */
export interface Capture {
  id: string;
  title: string;
  body: string;
  status: CaptureStatus;
  created_at: string;
}

/** The options of `arcl capture create`. */
export interface NewCapture {
  title: string;
  body?: string;
}

/**
 * A warning from the workspace sync around a saved change, as the CLI
 * prints it under `--json`: `blocker_edge`, or `workspace_sync` when the
 * export after the change failed.
 */
export interface ChangeWarning {
  code: string;
  message: string;
}

/** What a saved change returns, with the warnings its syncs gave. */
export interface Changed<T> {
  value: T;
  warnings: ChangeWarning[];
}

/**
 * Where views read and change records. `arcl serve` backs it locally; another
 * backend, such as a claude.ai artifact's database, can stand in for it.
 *
 * Each change runs the `arcl task` or `arcl capture` command of the same
 * name, with its validation, and fails with a {@link DataError} carrying the CLI's error
 * code and message. A saved change can still carry warnings.
 */
export interface DataSource {
  /** Every task, with its readiness and milestone. Aborting `signal` stops the read. */
  work(signal?: AbortSignal): Promise<WorkList>;
  /** One task with its readiness, relations, handoff and evidence. Aborting `signal` stops the read. */
  task(id: string, signal?: AbortSignal): Promise<TaskDetail>;
  /** Every task's ID, title and status, as `arcl list --kind task` lists them. */
  taskIndex(): Promise<TaskSummary[]>;
  /** `arcl task create`. */
  createTask(input: NewTask): Promise<Changed<Task>>;
  /** `arcl task update <id>`. */
  updateTask(id: string, edit: TaskEdit): Promise<Changed<Task>>;
  /** `arcl task start <id>`, with `--force` when `force` is set. */
  startTask(id: string, options?: { force?: boolean }): Promise<Changed<Task>>;
  /** `arcl task park <id>`. */
  parkTask(id: string): Promise<Changed<Task>>;
  /** `arcl task unpark <id>`. */
  unparkTask(id: string): Promise<Changed<Task>>;
  /** `arcl task handoff <id> --note <note>`: record a resume note and park the task. */
  handOffTask(id: string, note: string): Promise<Changed<Task>>;
  /** `arcl task complete <id>`. */
  completeTask(id: string, options?: CompleteOptions): Promise<Changed<TaskCompletion>>;
  /** `arcl capture create`. */
  createCapture(input: NewCapture): Promise<Changed<Capture>>;
}

/** A failed read or change, with the CLI's error code and message. */
export class DataError extends Error {
  /** The CLI's error code, such as `not_found`. */
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "DataError";
    this.code = code;
  }
}
