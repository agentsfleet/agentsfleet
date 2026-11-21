import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import {DRAFT, FLEET, MESSAGE, MODEL, REVIEW} from "@/tests/helpers/draft-fixtures";

// Value: protects=the save flap writes only a fleet and stays visible until confirmed;
// fails_when=the save repeats or progress clears early; seam=real browser and native HTTP adapter; no_claim=live persistence.
for (const motion of ["no-preference", "reduce"] as const) test(`Save confirms one fleet save (${motion})`, async ({page, chatProbe}, info) => {
  const receipt = Promise.withResolvers<void>();
  const createdAt = new Date().toISOString();
  const writes: {path: string; body: unknown}[] = [];
  const errors: string[] = [];page.on("pageerror", error => errors.push(error.message));
  await page.route("**/live/v1/workspaces/**", async route => {
    const request = route.request();const path = new URL(request.url()).pathname;
    if (request.method() === "POST") {
      writes.push({path, body: request.postDataJSON()});
      if (!path.endsWith("/fleets")) return route.fulfill({status: 404, json: {}});
      await receipt.promise;
      return route.fulfill({status: 201, json: {...FLEET, definition: REVIEW.effective.definition}});
    }
    if (path.endsWith("/models")) return route.fulfill({json: {items: [MODEL], next_cursor: null}});
    if (path.endsWith("/drafts")) return route.fulfill({json: {items: [], next_cursor: null}});
    if (path.endsWith("/messages")) return route.fulfill({json: {items: [{...MESSAGE, created_at: createdAt}, {...MESSAGE, created_at: createdAt, sequence: "2", role: "assistant", text: "Your setup is ready."}], next_cursor: null}});
    if (path.endsWith(`/drafts/${DRAFT.id}`)) return route.fulfill({json: DRAFT});
    if (path.endsWith(`/fleets/${FLEET.id}`)) return route.fulfill({json: FLEET});
    return route.fulfill({json: {items: [], next_cursor: null}});
  });
  try {
    await page.emulateMedia({reducedMotion: motion});
    await page.goto(`${chatProbe}?chatError=1&fleetSave=1`);
    await expect(page.getByRole("button", {name: "Save fleet", exact: true})).toHaveCount(0);
    await page.getByRole("button", {name: "Save", exact: true}).evaluate((button: HTMLButtonElement) => {button.click();button.click();});
    const progress = page.getByRole("status", {name: "Saving setup", exact: true});
    await expect(progress.locator("[data-chat-spinner]")).toBeVisible();
    await expect(progress.locator("[data-chat-spinner]")).toHaveCSS("animation-name", motion === "reduce" ? "none" : "spin");
    await expect(progress.locator("[data-chat-loading]")).toHaveCSS("animation-name", motion === "reduce" ? "none" : "chat-status-sweep");
    if (motion === "no-preference") {
      const rotation = await progress.locator("[data-chat-spinner]").evaluate(element => getComputedStyle(element).transform);
      await expect.poll(() => progress.locator("[data-chat-spinner]").evaluate(element => getComputedStyle(element).transform)).not.toBe(rotation);
    }
    await expect(progress.locator("[data-chat-loading]")).toHaveText("Saving");
    await expect(progress).toContainText("2s");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.getByText("Setup saved · v1", {exact: true})).toHaveCount(0);
    await page.screenshot({path: info.outputPath("fleet-save-progress.png"), scale: "css", animations: "disabled"});
    receipt.resolve();
    await expect(page.getByRole("heading", {name: "Setup saved · v1", exact: true})).toBeVisible();
    await expect(progress).toHaveCount(0);
    await expect(page.getByRole("link", {name: "Open fleet", exact: true})).toBeVisible();
    expect(writes).toEqual([{path: expect.stringMatching(/\/fleets$/), body: expect.objectContaining({name: REVIEW.effective.instance_name, start: {kind: "none"}})}]);
    expect(errors).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath("fleet-save-confirmed.png"), scale: "css", animations: "disabled"});
  } finally {receipt.resolve();}
});

// Value: protects=revision retries keep the same request after an unconfirmed receipt;
// fails_when=retry clones the fleet, changes its key/body or announces success early; seam=native client/browser HTTP; no_claim=live persistence.
test("an unconfirmed revision retries the original save and keeps the existing fleet", async ({page, chatProbe}, info) => {
  const draft = {...DRAFT, linked_fleet_id: FLEET.id, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
  const attempts: {path: string; body: unknown; key: string | undefined}[] = [];
  let committed = false;
  await page.route("**/live/v1/workspaces/**", async route => {
    const request = route.request();const path = new URL(request.url()).pathname;
    if (request.method() === "POST") {
      attempts.push({path, body: request.postDataJSON(), key: request.headers()["idempotency-key"]});
      if (!path.endsWith(`/fleets/${FLEET.id}/revisions`)) return route.fulfill({status: 400, json: {}});
      if (attempts.length === 1) {committed = true;return route.abort("failed");}
      return route.fulfill({json: {fleet_id: FLEET.id, setup_revision: "2", setup_etag: '"two"', etag: '"two"', schedules: []}});
    }
    if (path.endsWith("/models")) return route.fulfill({json: {items: [MODEL], next_cursor: null}});
    if (path.endsWith("/drafts")) return route.fulfill({json: {items: [], next_cursor: null}});
    if (path.endsWith("/messages")) return route.fulfill({json: {items: [], next_cursor: null}});
    if (path.endsWith(`/drafts/${DRAFT.id}`)) return route.fulfill({json: draft});
    if (path.endsWith(`/fleets/${FLEET.id}`)) return route.fulfill({json: {...FLEET, setup_revision: committed ? "2" : "1", definition: REVIEW.effective.definition}});
    return route.fulfill({json: {items: [], next_cursor: null}});
  });
  await page.goto(`${chatProbe}?chatError=1&fleetSave=1`);
  await page.getByRole("textbox").fill("Keep my unsent instruction");
  await page.getByRole("button", {name: "Save", exact: true}).click();
  await expect(page.getByText("Update not confirmed", {exact: true})).toBeVisible();
  await expect(page.getByText("Setup saved · v2", {exact: true})).toHaveCount(0);
  await page.getByRole("button", {name: "Retry", exact: true}).click();
  await expect(page.getByRole("heading", {name: "Setup saved · v2", exact: true})).toBeVisible();
  await expect(page.getByTestId("fleet-chat-footer")).toContainText("Setup saved · v2");
  await expect(page.getByRole("link", {name: "Open fleet", exact: true})).toHaveAttribute("href", new RegExp(`/fleets/${FLEET.id}$`));
  expect(attempts).toHaveLength(2);
  expect(attempts[0]?.key).toBeTruthy();
  expect(attempts[1]).toEqual(attempts[0]);
  await expect(page.getByRole("textbox")).toHaveValue("Keep my unsent instruction");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.screenshot({path: info.outputPath("fleet-revision-recovered.png"), scale: "css", animations: "disabled"});
});
