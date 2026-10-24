/**
 * The one module views read records through. Views call {@link dataSource};
 * a build that reads from somewhere other than `arcl serve` calls
 * {@link setDataSource} before the first view loads.
 */
import { serveSource } from "./serve";
import type { DataSource } from "./types";

export { latestRead, type LatestRead, type ReadReport } from "./latest";
export { serveSource, type ServeOptions } from "./serve";
export * from "./types";

let current: DataSource | null = null;

/** The data source views read from: `arcl serve` unless another was set. */
export function dataSource(): DataSource {
  current ??= serveSource();
  return current;
}

/** Make `source` the data source views read from. */
export function setDataSource(source: DataSource): void {
  current = source;
}
