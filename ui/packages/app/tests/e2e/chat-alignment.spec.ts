import { test } from "./fixtures/chat-probe";
import { expect } from "@playwright/test";
import { FLEET_DETAIL } from "@/tests/helpers/draft-fixtures";

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

test("the fleet disclosure retains loaded pages when collapsed and reopened", async ({page, chatProbe}, info) => {
  const mobile = info.project.name === "mobile-chromium";
  await page.setViewportSize(mobile ? {width: 390, height: 844} : {width: 1480, height: 1000});
  const first = {...FLEET_DETAIL, name: "Pull request review", description: "Reviews incoming pull requests", status: "active"};
  const next = {...first, id: "0199a801-1000-7000-8000-000000000008", name: "Support triage"};
  await page.route("**/live/v1/workspaces/*/fleets?*", route => route.fulfill({json: {
    items: new URL(route.request().url()).searchParams.has("starting_after") ? [next] : [first],
    total: null, next_cursor: new URL(route.request().url()).searchParams.has("starting_after") ? null : "next-fleet",
  }}));
  await page.goto(`${chatProbe}?setupAlignment=1`);
  await page.getByRole("button", {name: mobile ? "Open navigation" : "Expand navigation", exact: true}).click();
  const nav = page.getByRole("navigation", {name: mobile ? "Expanded navigation" : "Primary"});
  await expect(nav.getByRole("link", {name: "Pull request review", exact: true})).toBeVisible();
  await nav.getByRole("button", {name: "Load more fleets"}).click();
  await expect(nav.getByRole("link", {name: "Support triage", exact: true})).toBeVisible();
  const disclosure = nav.locator("summary").filter({hasText: "Fleets"});
  await disclosure.focus();
  await page.keyboard.press("Enter");
  await expect(nav.getByRole("link", {name: "Support triage", exact: true})).toBeHidden();
  await page.keyboard.press("Enter");
  await expect(nav.getByRole("link", {name: "Support triage", exact: true})).toBeVisible();
  await expect(nav.getByRole("button", {name: "Load more fleets"})).toHaveCount(0);
  await page.screenshot({path: info.outputPath("fleet-disclosure.png"), scale: "css", animations: "disabled"});
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
  await page.getByRole("button", {name: "Review setup", exact: true}).click();
  const review = page.getByRole("dialog", {name: "Fleet setup", exact: true});
  const purposeSize = await review.getByRole("region", {name: "Fleet purpose"}).locator("p").evaluate(element => getComputedStyle(element).fontSize);
  await expect(review.getByRole("region", {name: "Access and spending limits"}).getByRole("list")).toHaveCSS("font-size", purposeSize);
  await expect(review.getByRole("region", {name: "Fleet identity"})).toContainText("Pull request review");
  await expect(review.getByText("Secrets and connections:", {exact: true})).toBeVisible();
  const schedules = review.getByRole("region", {name: "Saved schedules", exact: true});
  await expect(schedules).toContainText("Scheduler unavailable");
  await expect(schedules).toContainText("Proposed timing changes take effect after saving");
  await expect(schedules.getByRole("button", {name: "Retry registration"})).toBeVisible();
  expect(await review.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await review.getByRole("region", {name: "Fleet identity"}).scrollIntoViewIfNeeded();
    await page.screenshot({path: info.outputPath(`fleet-setup-review-${theme}.png`), scale: "css", animations: "disabled"});
    await schedules.scrollIntoViewIfNeeded();
    await page.screenshot({path: info.outputPath(`fleet-setup-schedules-${theme}.png`), scale: "css", animations: "disabled"});
  }
  await page.keyboard.press("Escape");
  await expect(review).toBeHidden();
  await expect(page.getByRole("button", {name: "Work context"})).toHaveCount(0);
  await expect(input).toHaveValue("Preserve the existing regression assertions");
});
