import { test } from "./fixtures/chat-probe";
import { expect } from "@playwright/test";
import { FLEET_DETAIL, MODEL } from "@/tests/helpers/draft-fixtures";

test("fleet Summary is inline and the composer keeps the approved horizontal alignment", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?alignment=1`);
  const thread = page.getByRole("region", {name: "Fleet chat", exact: true});
  const viewport = thread.getByRole("presentation").filter({has: page.getByRole("log")});
  await expect(viewport).toHaveCSS("overflow-y", "auto");
  await expect(viewport).toHaveCSS("min-height", "0px");
  await expect(thread.getByRole("button", {name: "Jump to latest", includeHidden: true})).toHaveCSS("position", "absolute");
  await expect(thread).toHaveCSS("flex-direction", "column");
  await expect(thread).toHaveCSS("min-height", "0px");

  await expect(page.getByRole("link", {name: "Back to your fleets", exact: true})).toHaveCount(0);
  if (info.project.name !== "mobile-chromium") {
    await expect(page.getByRole("navigation", {name: "Primary"})).toBeVisible();
    await expect(page.locator("summary").filter({hasText: "Your Fleets"})).toBeVisible();
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
  let fleetReads = 0;
  await page.route("**/live/v1/workspaces/*/fleets?*", route => {
    fleetReads++;
    const more = new URL(route.request().url()).searchParams.has("starting_after");
    return route.fulfill({json: {items: more ? [next] : [first], total: null, next_cursor: more ? null : "next-fleet"}});
  });
  await page.route("**/live/v1/workspaces/*/drafts?*", route => route.fulfill({json: {items: [], next_cursor: null}}));
  await page.goto(`${chatProbe}?setupAlignment=1`);
  if (mobile) {
    const open = page.getByRole("button", {name: "Open navigation", exact: true});
    await expect(open).toBeVisible();
    expect(fleetReads).toBe(0);
    await open.click();
  }
  const nav = page.getByRole("navigation", {name: mobile ? "Expanded navigation" : "Primary"});
  const fleet = nav.getByRole("link", {name: "Pull request review", exact: true});
  const edit = nav.getByRole("link", {name: "Edit Pull request review", exact: true});
  await expect(fleet).toBeVisible();
  expect(fleetReads).toBe(1);
  await nav.getByRole("button", {name: "Load more fleets"}).click();
  await expect(nav.getByRole("link", {name: "Support triage", exact: true})).toBeVisible();
  const disclosure = nav.locator("summary").filter({hasText: "Your Fleets"});
  await expect(disclosure.locator("svg")).toHaveCount(1);
  await expect(fleet.locator("svg")).toHaveCount(0);
  await expect(edit.locator(":scope > svg:last-child")).toHaveAttribute("width", "14");
  await expect(fleet.locator("span").first()).toHaveCSS("font-weight", "600");
  const labelEdges = await Promise.all([disclosure, fleet, edit].map(row => row.locator("span").first().boundingBox()));
  expect(labelEdges.map(box => box?.x)).toEqual(Array(3).fill(labelEdges[0]?.x));
  await disclosure.focus();
  await page.keyboard.press("Enter");
  await expect(nav.getByRole("link", {name: "Support triage", exact: true})).toBeHidden();
  await page.keyboard.press("Enter");
  await expect(nav.getByRole("link", {name: "Support triage", exact: true})).toBeVisible();
  await expect(nav.getByRole("link", {name: "Edit Support triage", exact: true})).toBeVisible();
  await expect(nav.getByRole("button", {name: "Load more fleets"})).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.evaluate(value => document.documentElement.dataset.theme = value, theme);
    await expect(disclosure).toHaveCSS("color", theme === "dark" ? "rgb(163, 171, 183)" : "rgb(94, 104, 118)");
    await page.screenshot({path: info.outputPath(`fleet-history-${theme}.png`), animations: "disabled"});
  }
  await page.screenshot({path: info.outputPath("fleet-disclosure.png"), scale: "css", animations: "disabled"});
});

test("saved setup has the same composer placement, flat header, summary and return route", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?setupAlignment=1`);
  if (info.project.name !== "mobile-chromium") await expect(page.getByRole("navigation", {name: "Primary"})).toBeVisible();
  await expect(page.getByRole("button", {name: "Save fleet", exact: true})).toHaveCount(0);
  await expect(page.getByRole("button", {name: "Save and publish", exact: true})).toBeVisible();
  await expect(page.getByRole("combobox", {name: "Authoring model"})).toHaveText("MiniMax M3.1 Flash");
  await expect(page.getByRole("link", {name: "Back to fleet conversation"})).toHaveAttribute("href", /\/fleets\/0199a801-/);
  await expect(page.getByRole("button", {name: "Edit instructions"})).toHaveCount(0);
  await expect(page.getByText("Draft", {exact: true})).toBeVisible();
  const input = page.getByRole("textbox");
  await input.fill("Preserve the existing regression assertions");
  const summary = page.getByRole("complementary", {name: "Summary"});
  await expect(summary.getByRole("region", {name: "Repositories"})).toContainText("agentsfleet/linkwarden");
  await expect(summary.getByRole("region", {name: "Secrets"})).toContainText("github");
  await expect(summary.getByRole("region", {name: "Public internet access"})).toBeVisible();
  const packageLabel = summary.getByText("Enable package manager domains", {exact: true});
  const policyRow = packageLabel.locator("../..");
  await expect(policyRow).toHaveCSS("border-top-width", "1px");
  await expect(packageLabel).toHaveCSS("white-space", "normal");
  await expect(summary.getByText("Next wake-up", {exact: true})).toBeVisible();
  await expect(summary).toContainText("Scheduler unavailable");
  await expect(summary.getByRole("button", {name: "Retry registration"})).toBeVisible();
  await expect(page.getByRole("button", {name: "Review setup", exact: true})).toHaveCount(0);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(await summary.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await summary.getByRole("region", {name: "Repositories"}).scrollIntoViewIfNeeded();
    await page.screenshot({path: info.outputPath(`fleet-setup-summary-${theme}.png`), scale: "css", animations: "disabled"});
  }
  await expect(page.getByRole("button", {name: "Work context"})).toHaveCount(0);
  await expect(input).toHaveValue("Preserve the existing regression assertions");
});

test("chat load failures show one compact recovery action in the composer flap", async ({page, chatProbe}, info) => {
  const phone = info.project.name.includes("mobile");
  await page.setViewportSize({width: phone ? 390 : 1480, height: phone ? 844 : 900});
  let unavailable = true;
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.route("**/live/v1/workspaces/*/models?*", route => route.fulfill({status: unavailable ? 503 : 200,
    contentType: "application/json", body: JSON.stringify(unavailable ? {} : {items: [MODEL], next_cursor: null})}));
  await page.route("**/live/v1/workspaces/*/drafts?*", route => route.fulfill({status: 200,
    contentType: "application/json", body: JSON.stringify({items: [], next_cursor: null})}));
  for (const theme of ["dark", "light"]) {
    unavailable = true;
    await page.goto(`${chatProbe}?chatError=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    const footer = page.getByTestId("fleet-chat-footer");
    const message = footer.getByRole("alert");
    await expect(message).toContainText("We couldn’t load your model choices. Try again in a moment.");
    const flap = footer.getByRole("status", {name: "Setup status"});
    await expect(flap).toContainText("Setup needs attention");
    const retry = flap.getByRole("button", {name: "Retry", exact: true});
    await expect(retry).toBeVisible();
    await expect(page.getByRole("status", {name: "Chat error notice"})).toHaveCount(0);
    await expect(page.getByText("No chat model yet", {exact: true})).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`chat-error-recovery-${theme}.png`), scale: "css", animations: "disabled"});
    unavailable = false;
    await retry.click();
    await expect(message).toHaveCount(0);
    await expect(page.getByRole("combobox", {name: "Authoring model"})).toBeEnabled();
    await page.getByRole("textbox", {name: "What would you like your fleet to do?"}).fill("Review the callback regression");
    await expect(page.getByRole("button", {name: "New fleet from message", exact: true})).toBeEnabled();
  }
  expect(errors).toEqual([]);
});

for (const state of ["queued", "running"]) test(`${state} chat shows a spinner, a brightening label and seconds`, async ({page, chatProbe}, info) => {
  const errors: string[] = [];page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?loading=${state}`);
  const label = page.getByText(state === "queued" ? "On it" : "Thinking", {exact: true});
  await expect(label).toHaveAttribute("data-chat-loading", "true");
  const line = label.locator("..");
  await expect(line.locator("[data-chat-spinner]")).toBeVisible();
  const clock = line.locator("[data-waiting-elapsed]");
  await expect(clock).toHaveAttribute("aria-hidden", "true");
  const before = await clock.textContent();
  await expect.poll(() => clock.textContent()).not.toBe(before);
  await page.emulateMedia({reducedMotion: "no-preference"});
  await expect(label).toHaveCSS("animation-name", "chat-status-sweep");
  await page.screenshot({path: info.outputPath(`chat-${state}.png`), scale: "css"});
  await page.emulateMedia({reducedMotion: "reduce"});
  await expect(label).toHaveCSS("animation-name", "none");
  await expect(line.locator("[data-chat-spinner]")).toHaveCSS("animation-name", "none");
  expect(errors).toEqual([]);
});

test("context compaction uses one quiet progress line without a second thinking indicator", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?loading=compacting`);
  const label = page.getByText("Compacting context · This can take a few minutes", {exact: true});
  await expect(label).toHaveCount(1);
  const status = label.locator("..");
  await expect(status).toHaveAttribute("aria-live", "polite");
  await expect(status.locator("svg")).toBeVisible();
  await expect(page.getByText("Thinking", {exact: true})).toHaveCount(0);
  await page.emulateMedia({reducedMotion: "no-preference"});
  await expect(label).toHaveCSS("animation-name", "chat-status-sweep");
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    expect(await status.evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`chat-compacting-${theme}.png`), scale: "css", animations: "disabled"});
  }
  await page.emulateMedia({reducedMotion: "reduce"});
  await expect(label).toHaveCSS("animation-name", "none");
});
