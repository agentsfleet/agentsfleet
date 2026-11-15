import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import {DRAFT, FLEET, MESSAGE, MODEL, REVIEW} from "@/tests/helpers/draft-fixtures";

// Value: protects=the active save flap writes only a fleet and stays visible until confirmed;
// fails_when=publication is restored or progress clears early; seam=real browser and native HTTP adapter; no_claim=live persistence.
test("Save and publish confirms one fleet save without opening Plugin publication", async ({page, chatProbe}, info) => {
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
    await page.goto(`${chatProbe}?chatError=1&fleetSave=1`);
    await expect(page.getByRole("link", {name: "Plugins", exact: true})).toHaveCount(0);
    await expect(page.getByRole("button", {name: "Save fleet", exact: true})).toHaveCount(0);
    await page.getByRole("button", {name: "Save and publish", exact: true}).click();
    const progress = page.getByRole("status", {name: "Saving setup", exact: true});
    await expect(progress.locator("[data-chat-spinner]")).toBeVisible();
    await expect(progress.locator("[data-chat-loading]")).toHaveText("Saving and publishing");
    await expect(progress).toContainText("2s");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.getByText("Setup saved · v1", {exact: true})).toHaveCount(0);
    await page.screenshot({path: info.outputPath("fleet-save-progress.png"), scale: "css", animations: "disabled"});
    receipt.resolve();
    await expect(page.getByText("Setup saved · v1", {exact: true})).toBeVisible();
    await expect(progress).toHaveCount(0);
    await expect(page.getByRole("link", {name: "Open fleet", exact: true})).toBeVisible();
    expect(writes).toEqual([{path: expect.stringMatching(/\/fleets$/), body: expect.objectContaining({name: REVIEW.effective.instance_name, start: {kind: "none"}})}]);
    expect(errors).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath("fleet-save-confirmed.png"), scale: "css", animations: "disabled"});
  } finally {receipt.resolve();}
});
