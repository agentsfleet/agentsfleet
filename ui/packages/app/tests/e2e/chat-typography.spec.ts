import { test } from "./fixtures/chat-probe";
import { expect, type Locator, type Page } from "@playwright/test";
import { TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";
import { DRAFT } from "@/tests/helpers/draft-fixtures";
import { SCHEDULE_EVENT } from "@/tests/helpers/schedule-activity";

test("incoming progress preserves the draft and reading position until the reader jumps to latest", async ({page, chatProbe}) => {
  await page.goto(`${chatProbe}?scroll=1`);
  const input = page.getByRole("textbox");
  const viewport = page.locator('[data-testid="fleet-thread-root"] [role="presentation"]');
  const draft = "Keep the login fix in the callback handler. Preserve session expiry.";
  for (const {atBottom, composing} of [{atBottom: true, composing: true}, {atBottom: false, composing: true}, {atBottom: false, composing: false}]) {
    await input.fill(composing ? draft : "");
    if (!composing) await input.blur();
    await viewport.evaluate((element, bottom) => {
      element.scrollTop = bottom ? element.scrollHeight : 0;
    }, atBottom);
    await expect.poll(() => viewport.evaluate(element => element.scrollHeight > element.clientHeight)).toBe(true);
    const samples = await viewport.evaluate(element => new Promise<number[]>(resolve => {
      const positions = [element.scrollTop];
      window.dispatchEvent(new Event("fixture:progress"));
      const sample = () => {
        positions.push(element.scrollTop);
        if (positions.length === 20) resolve(positions);
        else requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    }));
    expect(Math.max(...samples) - Math.min(...samples)).toBeLessThanOrEqual(1);
    await expect(input).toHaveValue(composing ? draft : "");
    if (composing) await expect(input).toBeFocused();
    else await expect(input).not.toBeFocused();
  }
  await page.getByRole("button", {name: "Jump to latest"}).click();
  await expect.poll(() => viewport.evaluate(element => element.scrollHeight - element.clientHeight - element.scrollTop)).toBeLessThanOrEqual(1);
  await expect(input).toHaveValue("");
  const updates = page.getByText("New recorded update: the callback regression is still running. Its result has not been confirmed.", {exact: true});
  const recorded = await updates.count();
  await page.evaluate(() => window.dispatchEvent(new Event("fixture:progress")));
  await expect(updates).toHaveCount(recorded + 1);
  await expect(updates.last()).toBeInViewport();
  await expect.poll(() => viewport.evaluate(element => element.scrollHeight - element.clientHeight - element.scrollTop)).toBeLessThanOrEqual(1);
  await expect(input).toHaveValue("");
});

test("conversation and composer use the shared 14px typography", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => {errors.push(error.message); console.error(error);});
  await page.goto(chatProbe);
  const answer = page.getByText("The request has an owner.", {exact: false});
  const user = page.getByText("Investigate the failing test and explain the evidence.", {exact: true});
  const input = page.getByRole("textbox");
  await expect(answer).toBeVisible();
  for (const text of [answer, user, input]) {
    await expect(text).toHaveCSS("font-size", "14px");
    await expect(text).toHaveCSS("line-height", "24.5px");
  }
  expect(await page.evaluate(async () => (await document.fonts.load('14px "Instrument Sans Variable"', "Typography")).length)).toBeGreaterThan(0);
  await input.fill("Keep the diagnosis concise and cite the source.");
  await expect(input).toBeFocused();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(errors).toEqual([]);
  const screenshot = info.outputPath("chat-14px-fixture.png");
  await page.screenshot({path: screenshot});
  await info.attach("chat-14px-fixture", {path: screenshot, contentType: "image/png"});
});

test("the pull request starter remains editable and readable before any setup request", async ({page, chatProbe}, info) => {
  const writes: string[] = [];
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => {
    if (!["GET", "HEAD"].includes(request.method())) writes.push(request.method());
  });
  await page.goto(`${chatProbe}?starters=1`);
  await expect(page.getByRole("heading", {name: "What should your fleet do?"})).toBeVisible();
  const suggestion = page.getByRole("button", {name: "Review pull requests", exact: true});
  const input = page.getByRole("textbox", {name: "Describe the work you want this fleet to do…"});
  await expect(input).toHaveValue("");
  const unfinished = page.getByRole("region", {name: "Unfinished setups"});
  const resume = unfinished.getByRole("link", {name: DRAFT.title});
  const remove = unfinished.getByRole("button", {name: `Delete chat: ${DRAFT.title}`});
  const composerBefore = await page.getByRole("form", {name: "Setup chat composer"}).boundingBox();
  const row = await resume.boundingBox();
  const action = await resume.getByText("Resume", {exact: true}).boundingBox();
  if (!composerBefore || !row || !action) throw new Error("Unfinished setup and composer geometry must be available");
  expect(Math.abs(row.x - composerBefore.x)).toBeLessThanOrEqual(1);
  expect(Math.abs(row.x + row.width - composerBefore.x - composerBefore.width)).toBeLessThanOrEqual(1);
  expect(Math.abs(row.x + row.width - action.x - action.width - 15)).toBeLessThanOrEqual(1);
  await resume.hover(); await expect(remove).toHaveCSS("opacity", "1");
  const removeBox = await remove.boundingBox();
  if (!removeBox) throw new Error("The delete control must remain reachable");
  expect(removeBox.x + removeBox.width).toBeLessThanOrEqual(action.x);
  expect(await resume.getByText("Resume", {exact: true}).boundingBox()).toEqual(action);
  await resume.focus(); await page.keyboard.press("Tab"); await expect(remove).toBeFocused();
  await suggestion.focus(); await page.keyboard.press("Enter");
  await expect(input).toHaveValue(/Does the review cover the requested pull request at its current head\?/);
  await expect(input).toHaveValue(/Are the findings supported by concrete code or check evidence\?/);
  const edited = `${await input.inputValue()}\n\nOnly review the API changes; preserve session expiry.`;
  await input.fill(edited);
  await expect(input).toHaveValue(edited);
  await expect(input).toHaveCSS("font-size", "14px");
  await expect(page.getByRole("button", {name: "Send message"})).toBeEnabled();
  expect(writes).toEqual([]);
  expect(errors).toEqual([]);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const composer = await page.getByRole("form", {name: "Setup chat composer"}).boundingBox();
  expect(composer).not.toBeNull();
  if (!composer) throw new Error("The starter must leave its editable composer visible");
  expect(composer.y + composer.height).toBeLessThanOrEqual((page.viewportSize()?.height ?? 0) + 1);
  const screenshot = info.outputPath("editable-pull-request-starter.png");
  await page.screenshot({path: screenshot});
  await info.attach("editable-pull-request-starter", {path: screenshot, contentType: "image/png"});
});

test("setup checks use enclosed icons and incoming messages have readable details", async ({page, chatProbe}, info) => {
  await page.emulateMedia({reducedMotion: "reduce"});
  await page.goto(`${chatProbe}?setup=1`);
  const checks = page.getByRole("list", {name: "Setup checks"});
  await expect(checks.getByText("GitHub credential", {exact: true})).toBeVisible();
  await expect(checks.getByText("GitHub connection", {exact: true})).toBeVisible();
  await expect(checks.getByText("GitHub approval", {exact: true})).toBeVisible();
  await expect(checks.locator("svg.lucide-circle-check")).toHaveCount(6);
  await expect(checks.locator("svg").first()).toHaveCSS("width", "14px");
  await expect(page.getByText("Message received", {exact: true})).toBeVisible();
  const disclosure = page.getByRole("button", {name: "View message"});
  await disclosure.focus(); await page.keyboard.press("Enter");
  await expect(page.getByText("Diagnose the failed title regression using the current source.")).toBeVisible();
  await expect(page.locator("pre, svg.lucide-braces, svg.lucide-check")).toHaveCount(0);
  const failure = page.getByRole("status").filter({hasText: "The response was interrupted."});
  await expect(failure).toBeVisible();
  await expect(failure).toHaveCSS("font-size", "14px");
  const details = failure.locator("..").getByRole("button", {name: "View details"});
  await expect(page.getByText("The connection to the model provider was lost mid-turn.")).not.toBeVisible();
  await details.focus(); await page.keyboard.press("Enter");
  await expect(page.getByText("The connection to the model provider was lost mid-turn.")).toBeVisible();
  await page.keyboard.press("Enter");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const screenshot = info.outputPath("setup-message-clarity.png");
  await page.screenshot({path: screenshot});
  await info.attach("setup-message-clarity", {path: screenshot, contentType: "image/png"});
});

test("recorded actions stay visible and distinguish every terminal outcome", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?activity=1`);
  const group = page.getByRole("button", {name: /^Tool activity:/});
  await expect(group).toContainText("Opening pull request agentsfleet/linkwarden · 2 active");
  await expect(group).toHaveAccessibleName("Tool activity: Opening pull request agentsfleet/linkwarden · 2 active");
  await group.focus(); await page.keyboard.press("Enter");
  const tools = page.getByRole("list", {name: "Tool activity"});
  await expect(tools.getByText("Checked environment", {exact: true})).toBeVisible();
  await expect(tools.getByText("Read file", {exact: true})).toBeVisible();
  await expect(tools.getByText("Ran command", {exact: true})).toBeVisible();
  const pull = tools.locator('[data-tool="http_request"]').last();
  const action = pull.locator("button").first();
  await action.focus(); await page.keyboard.press("Enter");
  await expect(action).toHaveAttribute("aria-expanded", "true");
  await expect(pull.getByText("Input", {exact: true})).toBeVisible();
  await expectActivityGeometry(page, action, tools, info.project.name === "mobile-chromium");
  await page.emulateMedia({reducedMotion: "reduce"});
  for (const icon of await tools.locator('[data-tool-shimmer="true"]').all()) {
    await expect(icon).toHaveCSS("animation-name", "none");
    await expect(icon).toHaveCSS("color", "rgb(147, 197, 253)");
  }
  const screenshot = info.outputPath("chat-tool-activity-fixture.png");
  await page.screenshot({path: screenshot});
  await info.attach("chat-tool-activity-fixture", {path: screenshot, contentType: "image/png"});
  for (const [state, wording, color] of [["succeeded", "Opened pull request", "rgb(74, 222, 128)"], ["failed", "Failed: opening pull request", "rgb(252, 165, 165)"],
    ["interrupted", "Interrupted: opening pull request", "rgb(251, 191, 36)"], ["unconfirmed", "Outcome unavailable: opening pull request", "rgb(148, 160, 151)"]] as const) {
    await page.getByRole("button", {name: `Show ${state}`, exact: true}).click();
    await expect(pull.locator("[data-command-heading]")).toContainText(wording);
    await expect(pull.locator("[data-tool-bullet]")).toHaveCSS("color", color);
    if (state !== "succeeded") await expect(pull.locator("[data-command-heading]")).not.toContainText("Opened pull request");
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});

test("work stays in a sidebar while the ordinary chat composer preserves its draft", async ({page, chatProbe}, info) => {
  const work = [summary(), summary(TASK_IDS[1], "running"), summary(TASK_IDS[2], "ready")];
  await page.route("**/live/v1/workspaces/workspace/fleets/linkwarden/tasks**", async route => {
    expect(route.request().headers()["authorization"]).toBeUndefined();
    const pathname = new URL(route.request().url()).pathname;
    const task = work.find(task => pathname.endsWith(task.id));
    const saved = task ? detail(task) : null;
    const target = task ? {kind: "existing_pr", repository: "linkwarden/linkwarden", number: Number(task.id.slice(-3)), branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: `https://github.com/linkwarden/linkwarden/pull/${Number(task.id.slice(-3))}`, workflows: []} : null;
    await route.fulfill({headers: {etag: `"${"a".repeat(64)}"`}, json: saved ? {...saved, context: {...saved.context, target}} : {items: work, next_cursor: null}});
  });
  await page.goto(`${chatProbe}?tasks=1`);
  const input = page.getByRole("textbox", {name: /^Message your fleet/});
  await input.fill("Keep the redirect fix in the callback handler.");
  for (const window of [{width: 1280, height: 800}, {width: 1280, height: 360}, {width: 390, height: 667}]) {
    await page.setViewportSize(window);
    await expect(page.getByRole("link", {name: /^Pull request #123 ·/})).toHaveAttribute("href", "https://github.com/linkwarden/linkwarden/pull/123");
    if (window.width < 1024) await page.getByRole("button", {name: "Work context", exact: true}).click();
    const panel = page.getByRole("region", {name: "Current work"});
    await expect(panel.getByRole("heading", {name: "Durable tasks"})).toBeVisible();
    for (const section of ["Outputs", "Changes", "Sources"]) await expect(panel.getByRole("heading", {name: section, exact: true})).toBeAttached();
    const repair = panel.getByRole("button", {name: /PR #123 repair/});
    if (await repair.getAttribute("aria-expanded") !== "true") {await repair.focus(); await page.keyboard.press("Enter");}
    const selectedDetail = panel.getByLabel("PR #123 repair details", {exact: true});
    await expect(selectedDetail.getByText("What counts as done?", {exact: true})).toBeVisible();
    await expect(selectedDetail.getByText("Repair the callback redirect without weakening the check.", {exact: true})).toBeVisible();
    expect(await panel.evaluate(node => !!node.closest(".ph-no-capture.ph-no-record"))).toBe(true);
    expect(await page.getByTestId("fleet-chat-footer").evaluate(node => !!node.querySelector('[aria-label="Current work"]'))).toBe(false);
    if (window.width < 1024) await page.keyboard.press("Escape");
    await expect(input).toHaveValue("Keep the redirect fix in the callback handler.");
    await expect(input).toHaveCSS("font-size", "14px");
    const composer = await page.getByRole("form", {name: "Chat composer"}).boundingBox();
    expect(composer).not.toBeNull();
    expect(composer!.y + composer!.height).toBeLessThanOrEqual(window.height + 1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  const screenshot = info.outputPath("fleet-sidebar-mobile.png");
  await page.screenshot({path: screenshot});
  await info.attach("fleet-sidebar-mobile", {path: screenshot, contentType: "image/png"});
});

test("fleet_schedule_activity_rows_show_confirmed_facts", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  const reads: string[] = []; page.on("request", request => {if (request.url().includes("/event-details?")) reads.push(request.url());});
  await page.goto(`${chatProbe}?schedules=1`);
  for (const fact of ["set up:", "registered with the scheduler:", "pause requested:", "removal requested:", "removed:"])
    await expect(page.getByText(`Schedule At 9:00 AM (Asia/Kolkata) ${fact}`, {exact: false})).toBeVisible();
  await expect(page.getByText(/could not sync:.*Scheduler refused. Will retry on the next sync./)).toBeVisible();
  await expect(page.getByText("Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉")).toHaveCount(3);
  await expect(page.getByText("Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉").first()).toHaveCSS("font-size", "14px");
  await expect(page.getByText("Support queue reviewed.", {exact: true})).toBeVisible();
  await expect(page.getByText(/Ran out of memory/)).toBeVisible();
  await expect(page.getByRole("button", {name: "Run details"})).toHaveCount(2);
  await expect(page.locator("pre")).toHaveCount(0);
  expect(reads).toEqual([]);
  const details = page.getByRole("button", {name: "Run details"}).first();
  await expect(details).toHaveCSS("font-size", "14px");
  await details.focus(); await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", {name: "Event details"});
  await expect(dialog.getByText("Native recorded support review.", {exact: true})).toBeVisible();
  const address = new URL(reads[0]!);
  expect(Object.fromEntries(address.searchParams)).toEqual({workspace: SCHEDULE_EVENT.workspace_id, fleet: SCHEDULE_EVENT.fleet_id, event: SCHEDULE_EVENT.event_id});
  await page.keyboard.press("Escape"); await expect(dialog).not.toBeVisible();
  expect(errors).toEqual([]);
  await info.attach("schedule-activity", {body: await page.screenshot({fullPage: true}), contentType: "image/png"});
});

test("saved_schedule_summaries_and_activity_survive_reload", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?schedules=1`);
  const summary = page.getByRole("region", {name: "Schedules"});
  await expect(summary.getByText("At 9:00 AM (Asia/Kolkata) · Registration pending", {exact: true})).toBeVisible();
  await expect(summary.getByText("Review support failures — 東京 🦉", {exact: true})).toHaveCSS("font-size", "14px");
  await expect(page.getByText(/Save the fleet to activate it/)).toHaveCount(0);
  await page.getByRole("button", {name: "Retry registration"}).click();
  await expect(summary.getByText("At 9:00 AM (Asia/Kolkata) · Active", {exact: true})).toBeVisible();
  await expect(summary.getByRole("button", {name: "Retry registration"})).toHaveCount(0);
  await expect(page.getByText(/could not sync:.*Will retry on the next sync/)).toBeVisible();
  await page.getByRole("button", {name: "Deliver queued fire"}).click();
  await expect(page.getByText("Queued support review completed.", {exact: true})).toBeVisible();
  await expect(page.getByRole("button", {name: "Run details"})).toHaveCount(3);
  await page.reload();
  await expect(page.getByText("Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉")).toHaveCount(3);
  await expect(page.getByText(/registered with the scheduler:/)).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await info.attach("saved-schedule-reload", {body: await page.screenshot({fullPage: true}), contentType: "image/png"});
});

async function expectActivityGeometry(page: Page, action: Locator, tools: Locator, mobile: boolean) {
  for (const icon of await page.locator('[data-tool-bullet], [data-signal-spinner], [data-testid="fleet-message-sender"] svg').all()) {
    await expect(icon).toHaveCSS("width", "14px");
    await expect(icon).toHaveCSS("height", "14px");
  }
  await expect(action).toHaveCSS("outline-style", "solid");
  await expect(action).toHaveCSS("outline-width", "2px");
  await expect(action).toHaveCSS("outline-offset", "-2px");
  await expect(action).toHaveCSS("border-top-width", "0px");
  await expect(action).toHaveCSS("padding-left", "8px");
  await expect(action).toHaveCSS("padding-right", "8px");
  for (const button of await tools.locator("[data-command-heading]").all()) {
    expect((await button.locator("..").boundingBox())?.height).toBeGreaterThanOrEqual(mobile ? 44 : 36);
  }
  const heading = action.locator("[data-command-heading]");
  await expect(heading.getByText("Opening pull request", {exact: true})).toHaveCSS("font-size", "14px");
  const verb = await heading.getByText("Opening pull request", {exact: true}).boundingBox();
  const target = await heading.getByText("agentsfleet/linkwarden", {exact: true}).boundingBox();
  expect(verb).not.toBeNull(); expect(target).not.toBeNull();
  if (mobile) expect(target!.y).toBeGreaterThan(verb!.y);
  else expect(Math.abs(target!.y + target!.height / 2 - verb!.y - verb!.height / 2)).toBeLessThan(1);
}
