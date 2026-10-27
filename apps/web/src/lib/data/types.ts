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
  /** Who claimed the task when starting it. Absent while nobody holds it. */
  assignee?: string;
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

/** A capture as `arcl list --kind capture` lists it, without its body. */
export interface CaptureSummary {
  id: string;
  title: string;
  status: CaptureStatus;
}

/** What `arcl capture promote` turns a capture into. */
export type PromotionTarget = "task" | "spec" | "note";

/**
 * The options of `arcl capture promote`. A title or body left out is the
 * capture's. `acceptance_criteria` applies to a spec; `spec`, `plan`,
 * `phase`, `parent` and `priority` apply to a task.
 */
export interface CapturePromotion {
  to: PromotionTarget;
  title?: string;
  body?: string;
  acceptance_criteria?: string;
  spec?: string;
  plan?: string;
  phase?: string;
  /** The parent task's ID. */
  parent?: string;
  priority?: TaskPriority;
}

/** The record a promotion created. */
export interface Promoted {
  kind: PromotionTarget;
  id: string;
  title: string;
}

/** A spec record, as `arcl spec create --json` returns it. */
export interface Spec {
  id: string;
  title: string;
  body: string;
  acceptance_criteria: string;
  status: string;
}

/** A plan record, as `arcl plan create --json` returns it. */
export interface Plan {
  id: string;
  /** The spec that owns the plan. */
  spec_id: string;
  title: string;
  body: string;
  status: ContainerStatus;
}

/** The options of `arcl spec create`. */
export interface NewSpec {
  title: string;
  body?: string;
  acceptance_criteria?: string;
}

/** The options of `arcl plan create`. */
export interface NewPlan {
  title: string;
  /** The ID of the spec that owns the plan. */
  spec: string;
  body?: string;
}

/** What `arcl plan apply --json` returns: the plan with every phase and task it owns. */
export interface PlanApplied {
  plan: Plan;
  phases: Milestone[];
  tasks: Task[];
}

/** One record in the planning hierarchy, as `arcl tree --json` returns it. */
export interface TreeNode {
  /** The record kind, such as `spec`, `plan`, `phase` or `task`. */
  kind: string;
  id: string;
  title: string;
  status: string;
  children: TreeNode[];
}

/**
 * What the Planning view shows: the planning hierarchy in `arcl tree` order,
 * and every task's readiness and milestone.
 */
export interface Planning {
  tree: TreeNode[];
  work: WorkList;
}

/**
 * A record's workspace file, as `GET /api/records/{id}/source` returns it:
 * the bytes a workspace export writes for the record.
 */
export interface RecordSource {
  id: string;
  /** The record kind, such as `task`. */
  kind: string;
  /** The file's path in the workspace, such as `tasks/<id>.md`. */
  path: string;
  /** TOML front matter between `+++` lines, then the Markdown body. */
  source: string;
  /** Names the file's content; a save from an older revision is refused. */
  revision: string;
}

/** A raw record edit: the whole file, and the revision it was edited from. */
export interface RecordSourceEdit {
  source: string;
  revision: string;
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
 * Where views read and change records. `arcl serve` backs it locally; a
 * claude.ai artifact's database stands in for it in the artifact build.
 *
 * Each change runs the `arcl task`, `arcl capture`, `arcl spec` or `arcl plan`
 * command of the same name, with its validation, and fails with a {@link DataError} carrying the CLI's error
 * code and message. A saved change can still carry warnings. A backend that
 * cannot run the command queues it and fails with the code {@link QUEUED}.
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
  /** The captures not yet promoted or discarded, oldest first. Aborting `signal` stops the read. */
  captures(signal?: AbortSignal): Promise<CaptureSummary[]>;
  /** One capture with its body. Aborting `signal` stops the read. */
  capture(id: string, signal?: AbortSignal): Promise<Capture>;
  /** `arcl capture promote <id>`. */
  promoteCapture(id: string, input: CapturePromotion): Promise<Changed<Promoted>>;
  /** `arcl capture discard <id>`. */
  discardCapture(id: string): Promise<Changed<Capture>>;
  /** The planning hierarchy with every task's readiness. Aborting `signal` stops the read. */
  planning(signal?: AbortSignal): Promise<Planning>;
  /** `arcl spec create`. */
  createSpec(input: NewSpec): Promise<Changed<Spec>>;
  /** `arcl plan create`. */
  createPlan(input: NewPlan): Promise<Changed<Plan>>;
  /** `arcl plan apply <id>`, with `document` as the TOML `--file` holds. */
  applyPlan(id: string, document: string): Promise<Changed<PlanApplied>>;
  /**
   * The workspace file of record `id`. Only a backend that can save it, as
   * `arcl serve` does, or queue its save, as the artifact build does, has
   * it; the views hide the raw editor otherwise.
   */
  recordSource?(id: string, signal?: AbortSignal): Promise<RecordSource>;
  /**
   * Replace record `id` with `edit.source`, checked as `arcl` checks a
   * workspace file on import, when the file still has `edit.revision`.
   * Answers with the saved file.
   */
  saveRecordSource?(id: string, edit: RecordSourceEdit): Promise<Changed<RecordSource>>;
  /**
   * Follow the last sync and the edits waiting for the next. Only a backend
   * that queues changes for an agent to apply, such as a claude.ai
   * artifact's, has it. Returns a function that stops following.
   */
  watch?(listener: (status: SyncStatus) => void): () => void;
  /** Remove a queued edit, such as one that failed, from the queue. */
  dismiss?(id: string): Promise<void>;
}

/**
 * The code of the {@link DataError} a change fails with when the backend
 * queued it instead of saving it. An agent applies it with `arcl` on the
 * next sync; until then the records do not change.
 */
export const QUEUED = "queued";

/** An edit queued for an agent to apply with `arcl web apply`. */
export interface QueuedEdit {
  id: string;
  /** The `arcl serve` mutation it runs, such as `discard_capture`. */
  op: string;
  /** The record it changes, if the operation names one. */
  target: string | null;
  /** The mutation's options, as its endpoint takes them. */
  input: Record<string, unknown>;
  created_at: string;
  /**
   * `queued` until an agent claims it, `applying` while the agent runs it,
   * or `failed` with the CLI's error.
   */
  status: "queued" | "applying" | "failed";
  error: { code: string; message: string } | null;
}

/** When the backend's records last synced, and what waits for the next sync. */
export interface SyncStatus {
  /** When the last sync ran, as RFC 3339, or `null` before the first. */
  syncedAt: string | null;
  /** The Git commit the last sync read. */
  commit: string | null;
  /** Changes when any record a view reads changes. */
  revision: string | null;
  /** Queued, applying and failed edits, oldest first. */
  edits: QueuedEdit[];
  /**
   * Why the page stopped following syncs and the queue, or `null` while it
   * follows them. Once set, only reloading the page follows them again.
   */
  stopped: { code: string; message: string } | null;
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
