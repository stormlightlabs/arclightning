import { describe, expect, it } from "vitest";
import { HASH_ROUTING, appHref, appUrl, appUrlHref } from "./route";

describe("app routes", () => {
  it("route on the path in the arcl serve build", () => {
    expect(HASH_ROUTING).toBe(false);
    expect(appHref("/tasks/t-1")).toBe("/tasks/t-1");
    const page = new URL("http://127.0.0.1:4000/?status=pending#top");
    const url = appUrl(page);
    expect([url.pathname, url.searchParams.get("status")]).toEqual(["/", "pending"]);
    url.searchParams.delete("status");
    expect(page.search).toBe("?status=pending");
    expect(appUrlHref(url)).toBe("/");
  });

  it("route on the hash in the artifact build", () => {
    expect(appHref("/tasks/t-1?action=start", true)).toBe("#/tasks/t-1?action=start");
    const url = appUrl(new URL("https://artifact.example/some/path/#/tasks/t%201?action=start"), true);
    expect([url.pathname, url.searchParams.get("action")]).toEqual(["/tasks/t%201", "start"]);
    url.searchParams.delete("action");
    expect(appUrlHref(url, true)).toBe("#/tasks/t%201");
    expect(appUrl(new URL("https://artifact.example/"), true).pathname).toBe("/");
  });
});
