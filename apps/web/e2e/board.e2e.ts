import { expect, test, type Page } from "@playwright/test";
import { serveScratch, stopAll, type Served } from "./serve";

let served: Served | undefined;

test.beforeEach(async ({ page }) => {
  served = undefined;
  served = await serveScratch();
  page.on("pageerror", (error) => {
    throw error;
  });
});

// `stopAll` also cleans up a `serveScratch` that timed out before `served` was set.
test.afterEach(async () => {
  await stopAll();
});
test.afterAll(async () => {
  await stopAll();
});

/** Create a task with the CLI and return its ID. */
function create(title: string): string {
  return served!.cli("task", "create", title).task.id;
}

/** The board column whose heading starts with `title`. */
function column(page: Page, title: string) {
  return page.getByRole("region", { name: "Board" }).getByRole("list", { name: new RegExp(`^${title}`) });
}

/** Replace the raw editor's text with `text`, as one input. */
async function replaceFile(page: Page, text: string) {
  const editor = page.getByRole("textbox", { name: "Record file" });
  await expect(editor).toContainText("title");
  await editor.click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText(text);
}

test("dragging a card to In progress starts the task in arcl", async ({ page }) => {
  const id = create("Board task");
  await page.goto(served!.url);
  await page.getByRole("button", { name: "Board" }).click();
  await expect(page).toHaveURL(/view=board/);
  const card = column(page, "Pending").getByRole("listitem").filter({ hasText: "Board task" });
  await expect(card).toBeVisible();
  await card.dragTo(column(page, "In progress"));
  await expect(column(page, "In progress").getByRole("listitem").filter({ hasText: "Board task" })).toBeVisible();
  await expect.poll(() => served!.cli("task", "show", id).task.status).toBe("in_progress");

  // A move no task command makes is refused, and arcl keeps the status.
  await column(page, "In progress").getByRole("listitem").filter({ hasText: "Board task" }).dragTo(column(page, "Pending"));
  await expect(page.getByRole("alert")).toContainText("No task command moves a task from In progress to Pending.");
  expect(served!.cli("task", "show", id).task.status).toBe("in_progress");

  // The board is remembered for this viewer: Work opens on it again.
  await page.keyboard.press("g");
  await page.keyboard.press("i");
  await expect(page.getByRole("heading", { level: 1, name: "Inbox" })).toBeVisible();
  await page.keyboard.press("g");
  await page.keyboard.press("w");
  await expect(page.getByRole("region", { name: "Board" })).toBeVisible();
  await expect(page).toHaveURL(/view=board/);
});

test("the raw file editor saves a valid edit through arcl and refuses an invalid one", async ({ page }) => {
  const id = create("Raw task");
  await page.goto(served!.url);
  await page.getByRole("link", { name: "Raw task" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Raw task" })).toBeVisible();

  await page.getByRole("button", { name: "Edit file" }).click();
  await expect(page.getByText(`tasks/${id}.md`)).toBeVisible();

  // An invalid file is refused with the CLI's error, and nothing changes.
  await replaceFile(page, `+++\nid = "${id}"\ntitle = \n+++\n`);
  await page.getByRole("button", { name: "Save file" }).click();
  await expect(page.getByRole("alert")).toContainText("front matter TOML is invalid");
  expect(served!.cli("task", "show", id).task.title).toBe("Raw task");

  // A valid file saves, and the view shows it.
  await replaceFile(
    page,
    `+++\nid = "${id}"\ntitle = "Edited raw"\nstatus = "pending"\npriority = "high"\nposition = 0\n+++\n\nWritten in the raw editor.\n`,
  );
  await page.getByRole("button", { name: "Save file" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Edited raw" })).toBeVisible();
  await expect(page.getByText("Written in the raw editor.")).toBeVisible();
  const saved = served!.cli("task", "show", id).task;
  expect([saved.title, saved.priority, saved.body]).toEqual(["Edited raw", "high", "Written in the raw editor."]);
});
