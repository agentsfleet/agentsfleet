import { test } from "./fixtures/chat-probe";
import { expect } from "@playwright/test";

test("fleet Summary is inline and the composer keeps the approved horizontal alignment", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?alignment=1`);
  await expect(page.getByRole("link", {name: "Back to your fleets", exact: true})).toHaveCount(0);
  if (info.project.name !== "mobile-chromium") {
    await expect(page.getByRole("navigation", {name: "Primary"})).toBeVisible();
    await expect(page.getByRole("link", {name: "Your fleets", exact: true})).toBeVisible();
  }
  const input = page.getByRole("textbox");
  await input.fill("Keep my exact unsent instruction");
  const model = page.locator('[data-composer-part="model"]');
  const send = page.getByRole("button", {name: "Send", exact: true});
  await expect(model).toHaveText("MiniMax M3.1 Flash");
  const modelBox = await model.boundingBox();
  const sendBox = await send.boundingBox();
  expect(modelBox).not.toBeNull(); expect(sendBox).not.toBeNull();
  expect(Math.abs(modelBox!.y + modelBox!.height / 2 - sendBox!.y - sendBox!.height / 2)).toBeLessThan(8);
  expect(sendBox!.x - modelBox!.x - modelBox!.width).toBeGreaterThan(16);
  await page.getByRole("button", {name: "Fleet actions", exact: true}).click();
  const edit = page.getByRole("menuitem", {name: "Edit setup", exact: true});
  await expect(edit.locator("svg")).toBeVisible();
  await expect(edit).toHaveAttribute("href", /fleets\/new\?fleet=/);
  await page.keyboard.press("Escape");
  await page.screenshot({path: info.outputPath("fleet-conversation.png"), scale: "css", animations: "disabled"});
  const tasks = page.getByRole("button", {name: /^Tasks/});
  await expect(tasks).toHaveAttribute("aria-expanded", "false");
  await tasks.click();
  await expect(page.getByRole("button", {name: /Validate the Linkwarden repair/})).toBeVisible();
  await expect(page.getByRole("button", {name: "Work context"})).toHaveCount(0);
  await expect(tasks).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByRole("complementary", {name: "Summary"})).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.screenshot({path: info.outputPath("fleet-summary.png"), scale: "css", animations: "disabled"});
  await expect(input).toHaveValue("Keep my exact unsent instruction");
});

test("saved setup has the same composer placement, pencil, summary and return route", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?setupAlignment=1`);
  if (info.project.name !== "mobile-chromium") await expect(page.getByRole("navigation", {name: "Primary"})).toBeVisible();
  await expect(page.getByRole("button", {name: "Save changes", exact: true})).toBeVisible();
  await expect(page.getByRole("combobox", {name: "Authoring model"})).toHaveText("MiniMax M3.1 Flash");
  await expect(page.getByRole("link", {name: "Back to fleet conversation"})).toHaveAttribute("href", /\/fleets\/0199a801-/);
  await expect(page.getByRole("button", {name: "Edit instructions"}).locator("svg")).toBeVisible();
  const input = page.getByRole("textbox");
  await input.fill("Preserve the existing regression assertions");
  await page.screenshot({path: info.outputPath("fleet-setup.png"), scale: "css", animations: "disabled"});
  await expect(page.getByRole("region", {name: "Fleet purpose"})).toBeVisible();
  await expect(page.getByRole("button", {name: "Work context"})).toHaveCount(0);
  await expect(input).toHaveValue("Preserve the existing regression assertions");
});
