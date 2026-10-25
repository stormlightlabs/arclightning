import { appHref } from "$lib/route";

/** The Work list query parameter that names the row to select on load. */
export const SELECTED = "selected";

/** The page that shows one task. */
export function taskHref(id: string): string {
  return appHref(`/tasks/${encodeURIComponent(id)}`);
}

/**
 * The Work list URL that selects `id`. `from` is the Work list's app URL,
 * as `appUrl` gives it, that the Task view was opened from, if it was, so
 * its filters come back too.
 */
export function workHref(id: string, from: URL | null = null): string {
  const search = new URLSearchParams(from?.pathname === "/" ? from.search : "");
  search.set(SELECTED, id);
  return appHref(`/?${search}`);
}

/** The row a Work list URL asks to select, or `null`. */
export function selectedOf(search: URLSearchParams): string | null {
  return search.get(SELECTED) || null;
}
