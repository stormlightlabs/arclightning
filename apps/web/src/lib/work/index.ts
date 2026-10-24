/** The Work list's filtering, grouping and keyboard rules, kept apart from the view. */
export * from "./filters";
export * from "./grouping";
export * from "./keys";

/** The page that shows one task. */
export function taskHref(id: string): string {
  return `/tasks/${encodeURIComponent(id)}`;
}
