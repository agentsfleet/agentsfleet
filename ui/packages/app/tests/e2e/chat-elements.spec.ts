import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";

test("typed chat outcomes remain readable in dark and light appearance", async ({page, chatProbe}, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?visual=1`);
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
    await page.screenshot({path: screenshot});
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
