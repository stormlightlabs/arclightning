import { page } from "vitest/browser";
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import CopyId, { COPIED, COPY_BY_HAND } from "../components/CopyId.svelte";

const ID = "arcl-t-01M5VDD6RKGADMNXF7KH4H0J83";

/** Replace `navigator.clipboard` with `value` for one test. */
function setClipboard(value: unknown) {
  Object.defineProperty(navigator, "clipboard", { value, configurable: true });
}

afterEach(() => {
  // Drop the own property, so the prototype's getter shows again.
  delete (navigator as { clipboard?: unknown }).clipboard;
  window.getSelection()?.removeAllRanges();
});

describe("CopyId", () => {
  it("copies the full ID from the button and says Copied", async () => {
    const writeText = vi.fn(async () => {});
    setClipboard({ writeText });
    await render(CopyId, { id: ID });
    await page.getByRole("button", { name: "Copy ID" }).click();
    await expect.element(page.getByRole("status")).toHaveTextContent(COPIED);
    expect(writeText).toHaveBeenCalledWith(ID);
  });

  it("copies when the ID itself is clicked", async () => {
    const writeText = vi.fn(async () => {});
    setClipboard({ writeText });
    await render(CopyId, { id: ID });
    await page.getByText(ID).click();
    await expect.element(page.getByRole("status")).toHaveTextContent(COPIED);
    expect(writeText).toHaveBeenCalledWith(ID);
  });

  it("keeps the click from the element around it", async () => {
    setClipboard({ writeText: async () => {} });
    const outer = vi.fn();
    document.body.addEventListener("click", outer);
    try {
      await render(CopyId, { id: ID });
      await page.getByRole("button", { name: "Copy ID" }).click();
      await expect.element(page.getByRole("status")).toHaveTextContent(COPIED);
      expect(outer).not.toHaveBeenCalled();
    } finally {
      document.body.removeEventListener("click", outer);
    }
  });

  it("selects the ID and says to copy by hand when there is no clipboard", async () => {
    setClipboard(undefined);
    await render(CopyId, { id: ID });
    await page.getByRole("button", { name: "Copy ID" }).click();
    await expect.element(page.getByRole("status")).toHaveTextContent(COPY_BY_HAND);
    expect(window.getSelection()?.toString()).toBe(ID);
  });

  it("selects the ID and says to copy by hand when the write is refused", async () => {
    const writeText = vi.fn(async () => {
      throw new DOMException("Write permission denied.", "NotAllowedError");
    });
    setClipboard({ writeText });
    await render(CopyId, { id: ID });
    await page.getByRole("button", { name: "Copy ID" }).click();
    await expect.element(page.getByRole("status")).toHaveTextContent(COPY_BY_HAND);
    expect(writeText).toHaveBeenCalledWith(ID);
    expect(window.getSelection()?.toString()).toBe(ID);
  });

  it("drops the message when the ID changes", async () => {
    setClipboard({ writeText: async () => {} });
    const screen = await render(CopyId, { id: ID });
    await page.getByRole("button", { name: "Copy ID" }).click();
    await expect.element(page.getByRole("status")).toHaveTextContent(COPIED);
    await screen.rerender({ id: "arcl-t-OTHER" });
    await expect.element(page.getByText("arcl-t-OTHER")).toBeInTheDocument();
    expect(page.getByRole("status").element().textContent).toBe("");
  });

  it("keeps the newer message when an earlier copy finishes late", async () => {
    let finishFirst = () => {};
    const writeText = vi
      .fn<() => Promise<void>>()
      .mockImplementationOnce(() => new Promise((resolve) => (finishFirst = resolve)))
      .mockImplementationOnce(async () => {
        throw new DOMException("Write permission denied.", "NotAllowedError");
      });
    setClipboard({ writeText });
    await render(CopyId, { id: ID });
    const button = page.getByRole("button", { name: "Copy ID" });
    await button.click();
    await expect.poll(() => writeText.mock.calls.length).toBe(1);
    await button.click();
    await expect.element(page.getByRole("status")).toHaveTextContent(COPY_BY_HAND);
    finishFirst();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(page.getByRole("status").element().textContent).toBe(COPY_BY_HAND);
  });

  it("clears the message in the DOM before a repeat, so it is announced again", async () => {
    setClipboard({ writeText: async () => {} });
    await render(CopyId, { id: ID });
    const button = page.getByRole("button", { name: "Copy ID" });
    await button.click();
    const status = page.getByRole("status");
    await expect.element(status).toHaveTextContent(COPIED);
    const seen: string[] = [];
    const observer = new MutationObserver(() => seen.push(status.element().textContent ?? ""));
    observer.observe(status.element(), { childList: true, characterData: true, subtree: true });
    try {
      await button.click();
      await expect.poll(() => seen).toEqual(["", COPIED]);
    } finally {
      observer.disconnect();
    }
  });
});
