import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";

test("tool disclosures remain reachable inside the conversation at 320px", async ({page, chatProbe}, info) => {
  await page.setViewportSize({width: 320, height: 844});
  await page.goto(`${chatProbe}?visual=1`);
  const column = page.getByRole("log", {name: "Chat"});
  const group = page.getByRole("button", {name: /^Tool activity:/});
  await group.scrollIntoViewIfNeeded();
  const columnBox = (await column.boundingBox())!;
  const groupBox = (await group.boundingBox())!;
  const arrow = (await group.locator("svg").last().boundingBox())!;
  expect(groupBox.x).toBeGreaterThanOrEqual(columnBox.x - 1);
  expect(groupBox.x + groupBox.width).toBeLessThanOrEqual(columnBox.x + columnBox.width + 1);
  expect(arrow.x + arrow.width).toBeLessThanOrEqual(columnBox.x + columnBox.width + 1);
  await group.click();
  for (const cell of await page.getByRole("list", {name: "Tool activity", exact: true}).locator("li[data-tool]").all()) {
    const control = cell.getByRole("button", {expanded: false}).first();
    await control.scrollIntoViewIfNeeded();
    const box = (await control.boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(columnBox.x + columnBox.width + 1);
    await control.click();
    await expect(cell.getByRole("heading", {name: "Input", exact: true})).toBeVisible();
  }
  await page.screenshot({path: info.outputPath("narrow-tool-details.png"), scale: "css", animations: "disabled"});
});

test("typed chat outcomes remain readable in dark and light appearance", async ({page, chatProbe}, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?visual=1&code=1`);
  await page.getByRole("button", {name: "Completed", exact: true}).click();
  await page.getByRole("button", {name: /^Tool activity:/}).click();
  for (const cell of await page.locator("li[data-tool]").all()) {
    await cell.getByRole("button", {expanded: false}).first().click();
  }
  const command = page.getByLabel("Command result");
  await expect(command.locator("code")).toHaveText("trivy image --severity CRITICAL release:latest");
  await expect(page.locator('[data-tool="file_edit"] [data-diff="removed"]')).toContainText("ready: false");
  await expect(page.locator('[data-tool="file_edit"] [data-diff="added"]')).toContainText("ready: true");
  await expect(page.locator('[data-tool="custom_release_report"]')).toContainText("Recorded example result");
  const report = page.getByRole("link", {name: "the build report"});
  await expect(report).toHaveAttribute("href", "https://example.test/release");
  await expect(report).toHaveAttribute("rel", /noopener/);
  await expect(page.locator('pre code').filter({hasText: "ready: true"})).toBeAttached();
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await command.scrollIntoViewIfNeeded();
    const viewport = page.getByRole("log", {name: "Chat"});
    expect(await viewport.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const screenshot = info.outputPath(`typed-chat-${theme}.png`);
    await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
    await info.attach(`typed-chat-${theme}`, {path: screenshot, contentType: "image/png"});
  }
  expect(errors).toEqual([]);
});

test("native attachment preparation accepts a preview and shows its text receipt", async ({page, chatProbe}) => {
  const requests: string[] = [];
  await page.route("**/live/v1/workspaces/*/fleets/*/uploads", async route => {
    requests.push(route.request().method());
    expect(route.request().postDataBuffer()?.toString()).toContain('filename="notes.txt"');
    await route.fulfill({status: 200, contentType: "application/json", body: JSON.stringify({
      preview: true, stored: false,
      reply: "Upload preview accepted. The file was not read, stored, or analyzed.",
    })});
  });
  await page.goto(`${chatProbe}?visual=1`);
  await page.getByRole("button", {name: "Completed", exact: true}).click();
  const oversized = page.waitForEvent("filechooser");
  await page.getByRole("button", {name: "Attach a preview file"}).click();
  await (await oversized).setFiles({name: "large.bin", mimeType: "application/octet-stream", buffer: Buffer.alloc(5 * 1024 * 1024 + 1)});
  await expect(page.getByRole("alert")).toContainText("up to 5 MiB");
  expect(requests).toEqual([]);
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", {name: "Attach a preview file"}).click();
  await (await chooser).setFiles({name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("demo")});
  await expect(page.getByText("notes.txt", {exact: true})).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  expect(requests).toEqual([]);
  await page.getByRole("button", {name: "Send", exact: true}).click();
  await expect(page.getByText(/Upload preview · Not saved to fleet history/)).toBeVisible();
  await expect(page.getByText(/The file was not read, stored, or analyzed/)).toBeVisible();
  expect(requests).toEqual(["POST"]);
});

test("a native upload submission stays visible and a refusal returns its draft", async ({page, chatProbe}, info) => {
  const received = Promise.withResolvers<void>();
  const respond = Promise.withResolvers<void>();
  await page.route("**/live/v1/workspaces/*/fleets/*/uploads", async route => {
    received.resolve(); await respond.promise;
    await route.fulfill({status: 503, contentType: "application/json", body: JSON.stringify({error: "Upload unavailable"})});
  });
  await page.goto(`${chatProbe}?visual=1`);
  await page.getByRole("button", {name: "Completed", exact: true}).click();
  const replies = await page.locator('[data-role="assistant"]').count();
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", {name: "Attach a preview file"}).click();
  await (await chooser).setFiles({name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("demo")});
  const composer = page.getByRole("textbox", {name: "Message your fleet…"});
  await composer.fill("Review this preview");
  await page.getByRole("button", {name: "Send", exact: true}).click();
  try {
    await received.promise;
    await expect(page.getByText("Uploading…", {exact: true})).toBeVisible();
    await expect(page.getByRole("log", {name: "Chat"}).getByText("notes.txt", {exact: true})).toBeVisible();
    await expect(page.locator('[data-role="assistant"]')).toHaveCount(replies);
    await expect(page.getByRole("button", {name: "Remove attachment"})).toHaveCount(0);
    const screenshot = info.outputPath("upload-pending.png");
    await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
    await info.attach("upload-pending", {path: screenshot, contentType: "image/png"});
  } finally {respond.resolve();}
  await expect(page.getByText(/The file preview was not accepted/)).toBeVisible();
  await expect(composer).toHaveValue("Review this preview");
  const filename = page.getByRole("form", {name: "Chat composer"}).getByText("notes.txt", {exact: true});
  const filenameBox = await filename.boundingBox();
  expect(filenameBox).not.toBeNull();
  expect(filenameBox!.height).toBeLessThanOrEqual(24);
  expect(filenameBox!.width).toBeGreaterThanOrEqual(48);
  await expect(page.getByText(/Upload preview · Not saved to fleet history/)).toHaveCount(0);
  if (info.project.name === "chromium") {
    const heading = await page.getByRole("heading", {name: "Summary", exact: true}).boundingBox();
    const viewport = await page.getByTestId("fleet-thread-root").locator('[role="presentation"]').boundingBox();
    expect(heading).not.toBeNull(); expect(viewport).not.toBeNull();
    expect(heading!.y).toBeGreaterThanOrEqual(viewport!.y);
    expect(heading!.y + heading!.height).toBeLessThanOrEqual(viewport!.y + viewport!.height);
  }
  const screenshot = info.outputPath("upload-refused.png");
  await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
  await info.attach("upload-refused", {path: screenshot, contentType: "image/png"});
  await page.getByRole("button", {name: "Remove attachment"}).click();
  await expect(page.getByText("notes.txt", {exact: true})).toHaveCount(0);
});

test("archive and deletion confirmations lead to distinct readable sample states", async ({page, chatProbe}, info) => {
  const viewport = page.viewportSize();
  if (!viewport) throw new Error("The archive render check requires a fixed viewport");
  const headerStatusVisible = viewport.width >= 680;
  await page.goto(`${chatProbe}?visual=1`);
  await page.getByRole("button", {name: "Completed", exact: true}).click();
  for (const [action, title, filename] of [["Archive", "Archive fleet?", "archive"], ["Permanently delete", "Permanently delete fleet?", "delete"]] as const) {
    await page.getByRole("button", {name: "Fleet actions", exact: true}).click();
    await page.getByRole("menuitem", {name: action, exact: true}).click();
    const dialog = page.getByRole("alertdialog", {name: title});
    await expect(dialog).toBeVisible();
    const confirm = await dialog.getByRole("button", {name: action, exact: true}).boundingBox();
    expect(confirm).not.toBeNull();
    expect(confirm!.y + confirm!.height).toBeLessThanOrEqual(page.viewportSize()!.height);
    await page.screenshot({path: info.outputPath(`${filename}-confirmation.png`), scale: "css", animations: "disabled"});
    await dialog.getByRole("button", {name: "Cancel", exact: true}).click();
  }
  for (const [outcome, message] of [["archived", "Fleet archived"], ["deleted", "Fleet permanently deleted"]] as const) {
    await page.goto(`${chatProbe}?${outcome === "archived" ? "visual=1&state=completed&archived=1" : "resourcePages=1&resource=new&notice=deleted"}`);
    if (outcome === "archived") {
      await expect(page.getByLabel("Fleet status: archived")).toBeVisible({visible: headerStatusVisible});
      await expect(page.getByRole("textbox")).toHaveCount(0);
      await expect(page.getByRole("heading", {name: "Ready for your review", exact: true})).toBeVisible();
      for (const name of ["Approve", "Deny"]) await expect(page.getByRole("button", {name, exact: true})).toHaveCount(0);
      await expect(page.getByRole("button", {name: "Quote reply", exact: true})).toBeDisabled();
    } else {
      await expect(page.getByRole("heading", {name: "New fleet", exact: true})).toBeVisible();
      await expect(page.getByRole("textbox")).toBeVisible();
      await expect(page.getByTestId("fleet-thread-root")).toHaveCount(0);
    }
    const notice = page.getByTestId("fleet-notice");
    await expect(notice).toContainText(message);
    await expect(notice).toHaveAttribute("aria-hidden", "false");
    expect((await notice.boundingBox())!.width).toBeLessThanOrEqual(page.viewportSize()!.width - 24);
    await page.screenshot({path: info.outputPath(`${outcome}-state.png`), scale: "css", animations: "disabled"});
    await notice.getByRole("button", {name: "Dismiss notification"}).click();
    if (outcome === "archived") {
      await expect(page.getByLabel("Fleet status: archived")).toBeVisible({visible: headerStatusVisible});
      await expect(page.getByText("Archived · This fleet is permanently stopped. Its history is available to read.", {exact: true})).toBeVisible();
    } else await expect(page.getByRole("heading", {name: "New fleet", exact: true})).toBeVisible();
  }
});
