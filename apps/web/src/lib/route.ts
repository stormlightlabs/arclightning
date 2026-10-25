/**
 * App paths and the URLs that carry them.
 *
 * The `arcl serve` build routes on the URL's path. The artifact build routes
 * on its hash, `#/tasks/<id>?action=start`, since the artifact host serves
 * the page at a path of its own and may not let the page change it. Views
 * build links with {@link appHref} and read the path and query with
 * {@link appUrl}, so they work the same in both builds.
 */

/** Whether the router reads the app's path from the URL's hash, as in the artifact build. */
export const HASH_ROUTING = import.meta.env.VITE_ARCL_TARGET === "artifact";

/** Stands in for the page's origin, which a sandboxed artifact page does not have, under hash routing. */
const HASH_BASE = "http://app.invalid";

/** The link to the app path `path`, such as `/tasks/t-1?action=start`. */
export function appHref(path: string, hash = HASH_ROUTING): string {
  return hash ? `#${path}` : path;
}

/**
 * A copy of the app's own URL within the page URL `url`, whose `pathname`
 * and `search` are the app path and query. Under hash routing they come from
 * the hash, and the origin is a stand-in.
 */
export function appUrl(url: URL, hash = HASH_ROUTING): URL {
  return hash ? new URL(url.hash.slice(1) || "/", HASH_BASE) : new URL(url);
}

/** The link to the app URL `url`, as {@link appUrl} returns one. */
export function appUrlHref(url: URL, hash = HASH_ROUTING): string {
  return appHref(`${url.pathname}${url.search}`, hash);
}
