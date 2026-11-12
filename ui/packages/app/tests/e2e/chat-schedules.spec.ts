import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { SCHEDULE_EVENT } from "@/tests/helpers/schedule-activity";

test("fleet_schedule_activity_rows_show_confirmed_facts", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  const reads: string[] = []; page.on("request", request => {if (request.url().includes("/event-details?")) reads.push(request.url());});
  await page.goto(`${chatProbe}?schedules=1`);
  for (const fact of ["set up:", "registered with the scheduler:", "pause requested:", "removal requested:", "removed:"])
    await expect(page.getByText(`Schedule At 9:00 AM (Asia/Kolkata) ${fact}`, {exact: false})).toBeVisible();
  await expect(page.getByText(/could not sync:.*Scheduler refused. Will retry on the next sync./)).toBeVisible();
  await expect(page.getByText("Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉")).toHaveCount(3);
  await expect(page.getByText("Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉").first()).toHaveCSS("font-size", info.project.name === "mobile-chromium" ? "16px" : "13px");
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
  expect(Object.fromEntries(address.searchParams)).toEqual({workspace: SCHEDULE_EVENT.workspace_id, fleet: SCHEDULE_EVENT.fleet_id, event: SCHEDULE_EVENT.event_id});
  await page.keyboard.press("Escape"); await expect(dialog).not.toBeVisible();
  expect(errors).toEqual([]);
  await info.attach("schedule-activity", {body: await page.screenshot({fullPage: true}), contentType: "image/png"});
});

test("saved_schedule_summaries_and_activity_survive_reload", async ({page, chatProbe}, info) => {
  await page.goto(`${chatProbe}?schedules=1`);
  const summary = page.getByRole("region", {name: "Schedules"});
  await expect(summary.getByText("At 9:00 AM (Asia/Kolkata) · Registration pending", {exact: true})).toBeVisible();
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
  await expect(page.getByText("Schedule At 9:00 AM (Asia/Kolkata) fired: Review support failures — 東京 🦉")).toHaveCount(3);
  await expect(page.getByText(/registered with the scheduler:/)).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await info.attach("saved-schedule-reload", {body: await page.screenshot({fullPage: true}), contentType: "image/png"});
});

