import { QUEUED } from "$lib/data";

/** The props of the `Feedback` that shows a failed change. */
export interface FailureNotice {
  tone: "info" | "danger";
  title: string;
  message: string;
}

/**
 * How a view shows a change that did not save: as `title` with the CLI's
 * error, or, when the backend queued the change for the next sync, as a
 * notice that says so. A queued change has not happened yet, so the view
 * keeps showing the records as they are.
 */
export function failureNotice(failure: { code: string; message: string }, title: string): FailureNotice {
  if (failure.code === QUEUED) return { tone: "info", title: "Queued for the next sync", message: failure.message };
  return { tone: "danger", title, message: `${failure.message} (${failure.code})` };
}
