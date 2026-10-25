/**
 * The one module views read records through. Views call {@link dataSource};
 * a build that reads from somewhere other than `arcl serve` calls
 * {@link setDataSource} before the first view loads. The artifact build calls
 * {@link connectArtifact} instead.
 */
import { type ArtifactHost, artifactSource } from "./artifact";
import { serveSource } from "./serve";
import { DataError, type DataSource } from "./types";

export {
  artifactSource,
  QUEUED_MESSAGE,
  type ArtifactDb,
  type ArtifactHost,
  type ArtifactOptions,
  type ArtifactUser,
} from "./artifact";
export { latestRead, type LatestRead, type ReadReport } from "./latest";
export { serveSource, type ServeOptions } from "./serve";
export * from "./types";

/**
 * Whether this is the claude.ai artifact build, which reads the artifact's
 * database instead of `arcl serve`. `build:artifact` sets
 * `VITE_ARCL_TARGET=artifact`.
 */
export const ARTIFACT_BUILD = import.meta.env.VITE_ARCL_TARGET === "artifact";

/** What views show when the artifact page has no database to read. */
export const OPEN_IN_CLAUDE =
  "This page reads its project from a claude.ai artifact's database, which only claude.ai provides. Open the artifact in claude.ai to see the project.";

let current: DataSource | null = null;

/**
 * The data source views read from: `arcl serve` unless another was set. The
 * artifact build never reads `arcl serve`; before {@link connectArtifact}
 * finishes, every read fails with {@link OPEN_IN_CLAUDE}.
 */
export function dataSource(): DataSource {
  current ??= ARTIFACT_BUILD ? unavailableSource() : serveSource();
  return current;
}

/** Make `source` the data source views read from. */
export function setDataSource(source: DataSource): void {
  current = source;
}

/**
 * Make the artifact's database the data source, and resolve whether the page
 * has one. Without one, as on a copy of the page opened outside claude.ai,
 * every read and change fails with {@link OPEN_IN_CLAUDE}.
 */
export async function connectArtifact(host: ArtifactHost | undefined = globalThis.window?.claude): Promise<boolean> {
  const db = host ? await host.use("db").catch(() => null) : null;
  if (!host || !db) {
    setDataSource(unavailableSource());
    return false;
  }
  const user = await host.use("user").catch(() => null);
  setDataSource(artifactSource(db, { userId: () => user?.id() ?? Promise.resolve(null) }));
  return true;
}

/** The {@link DataSource} methods a backend may leave out, and `then`. */
const OPTIONAL = new Set(["watch", "dismiss", "recordSource", "saveRecordSource", "then"]);

/** A data source whose every read and change fails with {@link OPEN_IN_CLAUDE}. */
function unavailableSource(): DataSource {
  const fail = () => Promise.reject(new DataError("unavailable", OPEN_IN_CLAUDE));
  return new Proxy({} as DataSource, {
    // Optional methods stay missing, so views hide what they offer, and a
    // source is not a thenable.
    get: (_target, key) => (typeof key === "string" && OPTIONAL.has(key) ? undefined : fail),
  });
}
