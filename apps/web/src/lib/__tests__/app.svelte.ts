/**
 * Stand-ins for `$app/state` and `$app/navigation`, so a route renders in a
 * browser test without SvelteKit's router. Tests move between pages with
 * {@link navigate}, which runs the `afterNavigate` callbacks as the router does.
 */
import type { AfterNavigate } from "@sveltejs/kit";
import { vi } from "vitest";

const ORIGIN = "http://localhost";

/** The page a route reads from `$app/state`. */
export const page = $state({
  url: new URL("/", ORIGIN),
  params: {} as Record<string, string>,
  state: {} as App.PageState,
});

/** `goto`, which records where a route sent the browser. */
export const goto = vi.fn(async (_url: string | URL, _options?: { state?: App.PageState }) => {});

let callbacks: ((navigation: AfterNavigate) => void)[] = [];

/** `afterNavigate`, which keeps the callback for {@link navigate}. */
export function afterNavigate(callback: (navigation: AfterNavigate) => void): void {
  callbacks.push(callback);
}

/** Show `path` without a navigation, as the page a route first renders on. */
export function show(path: string): void {
  page.url = new URL(path, ORIGIN);
  const task = /^\/tasks\/([^/]+)$/.exec(page.url.pathname);
  page.params = task ? { id: decodeURIComponent(task[1]) } : {};
  page.state = {};
}

/** Run the `afterNavigate` callbacks for arriving at the shown page from `from`. */
export function arrive(from: string): void {
  const navigation = { from: { url: new URL(from, ORIGIN) }, to: { url: page.url }, type: "link" };
  for (const callback of callbacks) callback(navigation as unknown as AfterNavigate);
}

/** Navigate from the shown page to `path`. */
export function navigate(path: string): void {
  const from = page.url.href;
  show(path);
  arrive(from);
}

/** Forget the callbacks and calls from the last test. */
export function reset(): void {
  callbacks = [];
  goto.mockClear();
  show("/");
}
