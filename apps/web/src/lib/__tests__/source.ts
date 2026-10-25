import type { DataSource } from "$lib/data";

/**
 * A data source for route tests: `answers` stands in for the methods a test
 * uses, and every other method rejects. `calls` records each call to a
 * method in `answers` as `[name, ...args]`, without abort signals.
 */
export function fakeSource(answers: Partial<DataSource>): { source: DataSource; calls: unknown[][] } {
  const calls: unknown[][] = [];
  const source = new Proxy({} as DataSource, {
    get(_target, name: string) {
      const answer = (answers as Record<string, (...args: unknown[]) => unknown>)[name];
      if (!answer) return () => Promise.reject(new Error(`${name} is not used in this test`));
      return (...args: unknown[]) => {
        calls.push([name, ...args.filter((arg) => !(arg instanceof AbortSignal))]);
        return answer(...args);
      };
    },
  });
  return { source, calls };
}
