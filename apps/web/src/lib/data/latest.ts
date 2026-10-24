/** Where {@link latestRead} reports the read that finished. */
export interface ReadReport<T> {
  loaded(value: T): void;
  failed(error: unknown): void;
}

/** A read that a view starts, restarts and stops. */
export interface LatestRead {
  /** Abort the read in flight, if any, and start a new one. */
  start(): void;
  /** Abort the read in flight, if any. */
  stop(): void;
}

/**
 * Run `read` one at a time for a view. Each start aborts the read before it,
 * and stop aborts the one in flight, so a Retry or an unmount leaves no
 * request behind. Only the newest read that was not aborted reports.
 */
export function latestRead<T>(read: (signal: AbortSignal) => Promise<T>, report: ReadReport<T>): LatestRead {
  let controller: AbortController | null = null;
  return {
    start() {
      controller?.abort();
      const current = new AbortController();
      controller = current;
      read(current.signal).then(
        (value) => {
          if (!current.signal.aborted) report.loaded(value);
        },
        (error: unknown) => {
          if (!current.signal.aborted) report.failed(error);
        },
      );
    },
    stop() {
      controller?.abort();
      controller = null;
    },
  };
}
