import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { SCHEDULE_EVENT } from "@/tests/helpers/schedule-activity";

test("fleet_schedule_activity_rows_show_confirmed_facts", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  const reads: string[] = []; page.on("request", request => {if (request.url().includes("/event-details?")) reads.push(request.url());});
  await page.goto(`${chatProbe}?schedules=1`);
  await expect(page.getByText("Scheduled wake-up", {exact: true})).toBeVisible();
  await expect(page.getByText("Review support failures — 東京 🦉 · At 9:00 AM (Asia/Kolkata)", {exact: true})).toBeVisible();
  for (const fact of ["registered with the scheduler:", "pause requested:", "removal requested:", "removed:"])
    await expect(page.getByText(`Schedule At 9:00 AM (Asia/Kolkata) ${fact}`, {exact: false})).toBeVisible();
  await expect(page.getByText(/could not sync:.*Scheduler refused. Will retry on the next sync./)).toBeVisible();
  await expect(page.getByText("Scheduled wake-up: Review support failures — 東京 🦉")).toHaveCount(3);
  await expect(page.getByText("Scheduled wake-up: Review support failures — 東京 🦉").first()).toHaveCSS("font-size", info.project.name === "mobile-chromium" ? "16px" : "13px");
  await expect(page.getByText("Support queue reviewed.", {exact: true})).toBeVisible();
  await expect(page.getByText(/Ran out of memory/)).toBeVisible();
  await expect(page.getByRole("button", {name: "Run details"})).toHaveCount(2);
  await expect(page.locator("pre")).toHaveCount(0);
  expect(reads).toEqual([]);
  const details = page.getByRole("button", {name: "Run details"}).first();
  await expect(details).toHaveCSS("font-size", info.project.name === "mobile-chromium" ? "16px" : "13px");
  await details.focus(); await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", {name: "Event details"});
  await expect(dialog.getByText("Native recorded support review.", {exact: true})).toBeVisible();
  const address = new URL(reads[0]!);
  expect(Object.fromEntries(address.searchParams)).toEqual({workspace: SCHEDULE_EVENT.workspace_id, fleet: SCHEDULE_EVENT.fleet_id, event: SCHEDULE_EVENT.turn_id});
  await page.keyboard.press("Escape"); await expect(dialog).not.toBeVisible();
  expect(errors).toEqual([]);
  await info.attach("schedule-activity", {body: await page.screenshot({fullPage: true}), contentType: "image/png"});
});

test("saved_schedule_summaries_and_activity_survive_reload", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?schedules=1`);
  const summary = page.getByRole("region", {name: "Schedules"});
  await expect(summary.getByText("At 9:00 AM (Asia/Kolkata) · Registration failed", {exact: true})).toBeVisible();
  await expect(summary.getByText("Review support failures — 東京 🦉", {exact: true})).toHaveCSS("font-size", info.project.name === "mobile-chromium" ? "16px" : "13px");
  await expect(page.getByText(/Save the fleet to activate it/)).toHaveCount(0);
  await page.getByRole("button", {name: "Retry registration"}).click();
  await expect(summary.getByText("At 9:00 AM (Asia/Kolkata) · Active", {exact: true})).toBeVisible();
  await expect(summary.getByRole("button", {name: "Retry registration"})).toHaveCount(0);
  await expect(page.getByText(/could not sync:.*Will retry on the next sync/)).toBeVisible();
  await page.getByRole("button", {name: "Deliver queued fire"}).click();
  await expect(page.getByText("Queued support review completed.", {exact: true})).toBeVisible();
  await expect(page.getByRole("button", {name: "Run details"})).toHaveCount(3);
  await page.reload();
  await expect(page.getByText("Scheduled wake-up: Review support failures — 東京 🦉")).toHaveCount(3);
  await expect(page.getByText(/registered with the scheduler:/)).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await info.attach("saved-schedule-reload", {body: await page.screenshot({fullPage: true}), contentType: "image/png"});
});


test("native scheduled composer preserves chat through edit, pause, resume and deletion", async ({page, chatProbe}, info) => {
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  const {SCHEDULE} = await import("@/tests/helpers/draft-fixtures");
  let current = {...SCHEDULE, fleet_id: "linkwarden", source: "trigger", etag: '"wake-1"', description: "Every 30 minutes",
    next_run_at: new Date(Date.now() + 30 * 60_000).toISOString(), override: {timezone: SCHEDULE.timezone, status: "active"}};
  const writes: {method: string; body: unknown}[] = [];
  await page.route(`**/live/v1/workspaces/workspace/fleets/linkwarden/schedules/${SCHEDULE.schedule_id}`, async route => {
    const request = route.request();
    if (request.method() === "GET") {await route.fulfill({json: current});return;}
    if (request.method() === "DELETE") {
      writes.push({method: "DELETE", body: null});await route.fulfill({status: 204});return;
    }
    expect(request.headers()["if-match"]).toBe(current.etag);
    const body = request.postDataJSON();writes.push({method: request.method(), body});
    current = {...current, ...body.override, override: body.override, etag: `"wake-${writes.length + 1}"`,
      run_state: body.override.status === "paused" ? "paused" : "scheduled"};
    await route.fulfill({json: current});
  });
  await page.goto(`${chatProbe}?tasks=1&wakeups=1`);
  const footer = page.getByTestId("fleet-chat-footer");
  const input = page.getByRole("textbox", {name: /^Message your fleet/});
  await input.fill("Keep my unsent instruction");
  await expect(footer.getByRole("button", {name: "Edit wake-up"})).toBeVisible();
  await expect(page.getByRole("complementary", {name: "Summary"}).getByRole("heading", {name: "Next wake-up"})).toHaveCount(0);
  await page.screenshot({path: info.outputPath("native-scheduled-composer.png"), scale: "css", animations: "disabled"});
  await footer.getByRole("button", {name: "Edit wake-up"}).click();
  let editor = page.getByRole("dialog", {name: "Edit wake-up"});
  await editor.getByLabel("What should the fleet do?").fill("Discard this local edit");
  await editor.getByRole("button", {name: "Cancel", exact: true}).click();
  expect(writes).toHaveLength(0);
  await footer.getByRole("button", {name: "Pause wake-up"}).click();
  await expect(footer.getByText("Paused", {exact: true})).toBeVisible();
  await footer.getByRole("button", {name: "Resume wake-up"}).click();
  await expect(footer.getByRole("button", {name: "Pause wake-up"})).toBeEnabled();
  await footer.getByRole("button", {name: "Edit wake-up"}).click();
  editor = page.getByRole("dialog", {name: "Edit wake-up"});
  await editor.getByLabel("Schedule", {exact: true}).fill("*/15 * * * *");
  await editor.getByLabel("What should the fleet do?").fill("Check the pull request and report failures");
  await editor.getByRole("button", {name: "Save", exact: true}).click();
  await expect(editor).toBeHidden();
  expect(writes[2]?.body).toEqual({override: {timezone: SCHEDULE.timezone, status: "active", cron: "*/15 * * * *", message: "Check the pull request and report failures"}});
  await footer.getByRole("button", {name: "Delete wake-up"}).click();
  let confirm = page.getByRole("alertdialog", {name: "Delete wake-up?"});
  await confirm.getByRole("button", {name: "Cancel", exact: true}).click();
  expect(writes).toHaveLength(3);
  await footer.getByRole("button", {name: "Delete wake-up"}).click();
  confirm = page.getByRole("alertdialog", {name: "Delete wake-up?"});
  await confirm.getByRole("button", {name: "Delete wake-up", exact: true}).click();
  await expect(footer.getByRole("button", {name: "Edit wake-up"})).toHaveCount(0);
  expect(writes.map(write => write.method)).toEqual(["PATCH", "PATCH", "PATCH", "DELETE"]);
  await expect(input).toHaveValue("Keep my unsent instruction");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("empty and invalid wake-up edits retain the saved schedule and unsent chat", async ({page, chatProbe}) => {
  const {SCHEDULE} = await import("@/tests/helpers/draft-fixtures");
  const saved = {...SCHEDULE, fleet_id: "linkwarden", source: "trigger", etag: '"unchanged"'};
  const writes: unknown[] = [];
  await page.route(`**/live/v1/workspaces/workspace/fleets/linkwarden/schedules/${SCHEDULE.schedule_id}`, route => {
    if (route.request().method() === "GET") return route.fulfill({json: saved});
    writes.push(route.request().postDataJSON());
    return route.fulfill({status: 400, json: {user_message: "Choose a valid cron schedule."}});
  });
  await page.goto(`${chatProbe}?tasks=1&wakeups=1`);
  const chat = page.getByRole("textbox", {name: /^Message your fleet/});
  await chat.fill("Keep this instruction while I correct the schedule");
  await page.getByRole("button", {name: "Edit wake-up"}).click();
  const editor = page.getByRole("dialog", {name: "Edit wake-up"});
  for (const label of ["Schedule", "Timezone", "What should the fleet do?"]) {
    const field = editor.getByLabel(label, {exact: true});
    const original = await field.inputValue();
    await field.fill("");
    await editor.getByRole("button", {name: "Save", exact: true}).click();
    await expect(editor).toBeVisible();
    await expect(field).not.toHaveJSProperty("validationMessage", "");
    expect(writes).toHaveLength(0);
    await field.fill(original);
  }
  await editor.getByLabel("Schedule", {exact: true}).fill("not-a-cron");
  await editor.getByRole("button", {name: "Save", exact: true}).click();
  await expect(editor.getByRole("alert")).toContainText("Choose a valid cron schedule.");
  await expect(editor.getByLabel("Schedule", {exact: true})).toHaveValue("not-a-cron");
  await expect(editor.getByRole("button", {name: "Save", exact: true})).toBeEnabled();
  expect(writes).toEqual([{override: {cron: "not-a-cron"}}]);
  await editor.getByRole("button", {name: "Cancel", exact: true}).click();
  await expect(chat).toHaveValue("Keep this instruction while I correct the schedule");
});
