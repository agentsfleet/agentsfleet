import { test } from "./fixtures/chat-probe";
import { expect } from "@playwright/test";

test("fleet header opens the summary directly and model stays beside Send", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?alignment=1`);
  await expect(page.getByRole("link", {name: "Back to your fleets", exact: true})).toHaveCount(0);
  if (info.project.name !== "mobile-chromium") {
    await expect(page.getByRole("navigation", {name: "Primary"})).toBeVisible();
    await expect(page.getByRole("link", {name: "Your fleets", exact: true})).toBeVisible();
    await expect(page.getByRole("link", {name: "ci-repairer", exact: true})).toHaveAttribute("aria-current", "page");
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
  expect(sendBox!.x - modelBox!.x - modelBox!.width).toBeLessThan(24);
  const edit = page.getByRole("link", {name: "Edit setup", exact: true});
  await expect(edit.locator("svg")).toBeVisible();
  await expect(edit).toHaveText("");
  await expect(edit).toHaveAttribute("href", /fleets\/new\?fleet=/);
  await page.screenshot({path: info.outputPath("fleet-conversation.png")});
  await page.getByRole("button", {name: "Toggle fleet summary"}).click();
  await expect(page.getByRole("button", {name: /Validate the Linkwarden repair/})).toBeVisible();
  await expect(page.getByRole("button", {name: "Work context"})).toHaveCount(0);
  await expect(page.getByRole("heading", {name: "Durable tasks"})).toBeVisible();
  await page.screenshot({path: info.outputPath("fleet-summary.png")});
  if (info.project.name === "mobile-chromium") await page.getByRole("button", {name: "Close", exact: true}).click();
  else await page.getByRole("button", {name: "Toggle fleet summary"}).click();
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
  await page.screenshot({path: info.outputPath("fleet-setup.png")});
  await page.getByRole("button", {name: "Toggle fleet summary"}).click();
  await expect(page.getByRole("region", {name: "Fleet purpose"})).toBeVisible();
  await expect(page.getByRole("button", {name: "Work context"})).toHaveCount(0);
  if (info.project.name === "mobile-chromium") await page.getByRole("button", {name: "Close", exact: true}).click();
  else await page.getByRole("button", {name: "Toggle fleet summary"}).click();
  await expect(input).toHaveValue("Preserve the existing regression assertions");
});
