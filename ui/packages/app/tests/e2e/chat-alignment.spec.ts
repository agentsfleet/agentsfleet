import { test } from "./fixtures/chat-probe";
import { expect } from "@playwright/test";
import { DRAFT, FLEET_DETAIL, MODEL } from "@/tests/helpers/draft-fixtures";

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
  const edges = await Promise.all([disclosure.locator("span").first(), fleet.locator("span").first(),
    nav.getByRole("link", {name: "Secrets", exact: true}).locator("svg")].map(item => item.boundingBox()));
  for (const edge of edges) expect(edge?.x).toBe(edges[0]?.x);
  const [fleetBox, editBox, rail] = await Promise.all([fleet.boundingBox(), edit.boundingBox(), nav.boundingBox()]);
  if (!fleetBox || !editBox || !rail) throw new Error("The fleet and its edit action should have visible bounds.");
  expect(editBox.y + editBox.height / 2).toBeCloseTo(fleetBox.y + fleetBox.height / 2, 0);
  expect(editBox.x).toBeGreaterThan(fleetBox.x);
  expect(editBox.x + editBox.width).toBeLessThan(rail.x + rail.width);
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
  await expect(page.getByRole("button", {name: "Save", exact: true})).toBeVisible();
  await expect(page.getByRole("combobox", {name: "Setup model"})).toHaveText("MiniMax M3.1 Flash");
  await expect(page.getByRole("link", {name: "Back to fleet conversation"})).toHaveAttribute("href", /\/fleets\/0199a801-/);
  await expect(page.getByRole("button", {name: "Edit instructions"})).toHaveCount(0);
  await expect(page.getByText("Draft", {exact: true})).toBeVisible();
  const input = page.getByRole("textbox");
  await input.fill("Preserve the existing regression assertions");
  const summary = page.getByRole("complementary", {name: "Summary"});
  await expect(summary.getByRole("region", {name: "Repositories"})).toContainText("agentsfleet/linkwarden");
  await expect(summary.getByRole("region", {name: "Secrets"})).toContainText("github");
  await expect(summary.getByRole("region", {name: "Public internet access"})).toBeVisible();
  const packageLabel = summary.getByText("Packages", {exact: true});
  const policyRow = packageLabel.locator("../..");
  await expect(policyRow).toHaveCSS("border-top-width", "0px");
  await expect(packageLabel).toHaveCSS("white-space", "nowrap");
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

test("a private setup title edits in place and saves when focus leaves", async ({page, chatProbe}) => {
  await page.route("**/live/v1/workspaces/*/models?*", route => route.fulfill({json: {items: [MODEL], next_cursor: null}}));
  await page.route("**/live/v1/workspaces/*/drafts?*", route => route.fulfill({json: {items: [], next_cursor: null}}));
  await page.route(`**/live/v1/workspaces/*/drafts/${DRAFT.id}/messages?*`, route => route.fulfill({json: {items: [], next_cursor: null}}));
  let draft = DRAFT;
  await page.route(`**/live/v1/workspaces/*/drafts/${DRAFT.id}`, route => {
    if (route.request().method() === "PATCH") {
      expect(route.request().headers()["if-match"]).toBe(DRAFT.etag);
      draft = {...DRAFT, title: route.request().postDataJSON().title, etag: '"renamed"'};
    }
    return route.fulfill({json: draft});
  });
  await page.goto(`${chatProbe}?chatError=1&fleetSave=1`);
  const heading = page.getByRole("heading", {name: DRAFT.title, exact: true});
  await expect(heading).toBeVisible();
  const before = await heading.boundingBox();
  await heading.press("F2");
  const field = page.getByRole("textbox", {name: "Chat name", exact: true});
  await expect(field).toBeFocused();
  await expect(field).toHaveCSS("border-top-width", "0px");
  await expect(field).toHaveCSS("text-decoration-line", "underline");
  const editing = await field.boundingBox();
  expect(editing?.height ?? 0).toBeGreaterThan(0);
  expect(editing?.height ?? Infinity).toBeLessThanOrEqual((before?.height ?? 0) + 4);
  await field.fill("Release checklist");
  await page.getByRole("form", {name: "Setup chat composer"}).getByRole("textbox").click();
  await expect(page.getByRole("heading", {name: "Release checklist", exact: true})).toBeVisible();
});

test("a resumed setup retries model choices beside the picker on both appearances", async ({page, chatProbe}, info) => {
  const phone = info.project.name.includes("mobile");
  await page.setViewportSize({width: phone ? 390 : 1480, height: phone ? 844 : 900});
  let unavailable = true;
  let historyReads = 0;
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.route("**/live/v1/workspaces/*/models?*", route => route.fulfill({status: unavailable ? 503 : 200,
    contentType: "application/json", body: JSON.stringify(unavailable ? {} : {items: [MODEL], next_cursor: null})}));
  await page.route("**/live/v1/workspaces/*/drafts?*", route => {
    historyReads += 1;
    return route.fulfill({json: {items: [], next_cursor: null}});
  });
  await page.route("**/live/v1/workspaces/*/fleets?*", route => route.fulfill({json: {items: [], next_cursor: null, total: null}}));
  await page.route(`**/live/v1/workspaces/*/drafts/${DRAFT.id}`, route => route.fulfill({json: DRAFT}));
  await page.route(`**/live/v1/workspaces/*/drafts/${DRAFT.id}/messages?*`, route => route.fulfill({json: {items: [], next_cursor: null}}));
  for (const theme of ["dark", "light"]) {
    unavailable = true;
    await page.goto(`${chatProbe}?chatError=1&fleetSave=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    const composer = page.getByRole("form", {name: "Setup chat composer"});
    const input = composer.getByRole("textbox");
    const model = composer.getByRole("combobox", {name: "Setup model"});
    const message = composer.getByText("Couldn’t load models", {exact: true});
    const retry = composer.getByRole("button", {name: "Retry models", exact: true});
    await input.fill("Review the callback regression");
    await expect(model).toBeDisabled();
    await expect(message).toBeVisible();
    await expect(retry).toBeVisible();
    await expect(page.getByText("Setup needs attention", {exact: true})).toHaveCount(0);
    await expect(page.getByText("No chat model yet", {exact: true})).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`model-list-recovery-${theme}.png`), scale: "css", animations: "disabled"});
    const before = historyReads;
    unavailable = false;
    await retry.click();
    await expect(message).toHaveCount(0);
    await expect(model).toBeEnabled();
    await expect(input).toHaveValue("Review the callback regression");
    await expect(input).toBeFocused();
    await expect(composer.getByRole("button", {name: "Send message", exact: true})).toBeEnabled();
    expect(historyReads).toBe(before);
  }
  expect(errors).toEqual([]);
});

for (const state of ["queued", "running"]) test(`${state} chat shows three dots on one text line`, async ({page, chatProbe}, info) => {
  const errors: string[] = [];page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?loading=${state}`);
  const line = page.getByRole("status", {name: state === "queued" ? "On it" : "Thinking", exact: true});
  const dots = line.locator(".chat-reply-dots > span");
  await expect(dots).toHaveCount(3);
  await expect(dots.first()).toBeVisible();
  expect(await line.evaluate(element => element.getBoundingClientRect().height)).toBe(
    await line.evaluate(element => parseFloat(getComputedStyle(element).lineHeight)));
  await page.emulateMedia({reducedMotion: "no-preference"});
  await expect(dots.first()).toHaveCSS("animation-name", "chat-reply-dot");
  await page.screenshot({path: info.outputPath(`chat-${state}.png`), scale: "css"});
  await page.emulateMedia({reducedMotion: "reduce"});
  await expect(dots.first()).toHaveCSS("animation-name", "none");
  await page.emulateMedia({forcedColors: "active", reducedMotion: "no-preference"});
  await expect(dots.first()).toHaveCSS("forced-color-adjust", "none");
  await expect(dots.first()).toHaveCSS("animation-name", "none");
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
