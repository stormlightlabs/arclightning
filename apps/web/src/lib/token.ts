/** The request header `arcl serve` reads the per-run token from. */
export const TOKEN_HEADER = "X-Arcl-Token";

/** The query parameter that carries the token on the first page load. */
export const TOKEN_QUERY = "token";

/** The session storage key that holds the token for this tab. */
export const TOKEN_STORAGE_KEY = "arcl-token";

/**
 * Move the token from a page URL into `storage`.
 *
 * `arcl serve` prints a URL with the token in its query, and accepts a query
 * token only for the first page load. Returns the URL without the token for
 * the caller to put in the address bar, or `null` when the URL had none.
 */
export function adoptToken(url: URL, storage: Pick<Storage, "setItem">): URL | null {
  const token = url.searchParams.get(TOKEN_QUERY);
  if (token === null) return null;
  storage.setItem(TOKEN_STORAGE_KEY, token);
  const cleaned = new URL(url);
  cleaned.searchParams.delete(TOKEN_QUERY);
  return cleaned;
}

/** The headers that authenticate a request to `arcl serve`. */
export function tokenHeaders(storage: Pick<Storage, "getItem">): Record<string, string> {
  const token = storage.getItem(TOKEN_STORAGE_KEY);
  return token === null ? {} : { [TOKEN_HEADER]: token };
}
