import { describe, expect, it } from "vitest";
import { TOKEN_HEADER, TOKEN_STORAGE_KEY, adoptToken, tokenHeaders } from "./token";

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => void values.set(key, value),
  };
}

describe("adoptToken", () => {
  it("stores the token and drops it from the URL", () => {
    const storage = memoryStorage();
    const cleaned = adoptToken(new URL("http://127.0.0.1:4000/inbox?token=abc&view=all#top"), storage);
    expect(cleaned?.href).toBe("http://127.0.0.1:4000/inbox?view=all#top");
    expect(storage.values.get(TOKEN_STORAGE_KEY)).toBe("abc");
  });

  it("leaves a URL without a token alone", () => {
    const storage = memoryStorage();
    expect(adoptToken(new URL("http://127.0.0.1:4000/"), storage)).toBeNull();
    expect(storage.values.size).toBe(0);
  });
});

describe("tokenHeaders", () => {
  it("sends the stored token in the token header", () => {
    const storage = memoryStorage();
    expect(tokenHeaders(storage)).toEqual({});
    storage.setItem(TOKEN_STORAGE_KEY, "abc");
    expect(tokenHeaders(storage)).toEqual({ [TOKEN_HEADER]: "abc" });
  });
});
