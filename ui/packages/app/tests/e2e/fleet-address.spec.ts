import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { UUID_V7 } from "@/tests/fleet-thread/steer-copy";

// The open chat is the fixture's own fleet; `agentbob` is another fleet in its workspace.
const BOB = "0199a801-1000-7000-8000-0000000000b0";
const AT = "2026-11-19T00:00:00Z";

test("chat_sends_an_addressed_message_and_opens_the_named_fleet", async ({page, chatProbe}) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  const lookups: string[] = [];
  const posts: {path: string; body: unknown}[] = [];
  await page.route("**/live/v1/workspaces/*/fleets?*", async route => {
    lookups.push(new URL(route.request().url()).search);
    await route.fulfill({json: {items: [{id: BOB, name: "agentbob", description: null, status: "active",
      created_at: AT, updated_at: AT, events_processed: "0", budget_used_nanos: "0"}], total: null, next_cursor: null}});
  });
  await page.route("**/live/v1/workspaces/*/fleets/*/messages", async route => {
    posts.push({path: new URL(route.request().url()).pathname, body: route.request().postDataJSON()});
    await route.fulfill({status: 202, json: {status: "accepted", event_id: "0199a801-1000-7000-8000-0000000000e0", replayed: false}});
  });
  await page.goto(`${chatProbe}?address=1`);
  await page.getByRole("textbox", {name: "Message your fleet…"}).fill("@agentbob retry");
  await page.getByRole("button", {name: "Send", exact: true}).click();
  await expect(page.getByLabel("Opened fleet")).toHaveText(BOB);
  expect(lookups).toEqual(["?name=agentbob"]);
  expect(posts).toEqual([{path: `/live/v1/workspaces/${WORKSPACE}/fleets/${BOB}/messages`,
    body: {message: "retry", operation_id: expect.stringMatching(UUID_V7)}}]);
  // Nothing waits behind it: the send settled, so no unsent notice remains.
  await expect(page.getByRole("list", {name: "Unsent messages"})).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("the workspace picker inserts only active fleets and keeps the unsent words", async ({page, chatProbe}, info) => {
  const fleets = ["bob", "bob-archive", "chief-of-staff"].map((name, index) => ({
    id: BOB.slice(0, -1) + index, name, status: name.endsWith("archive") ? "archived" : "active",
    description: null, created_at: AT, updated_at: AT, events_processed: "0", budget_used_nanos: "0",
  }));
  let reads = 0;
  await page.route(`**/live/v1/workspaces/${WORKSPACE}/fleets?limit=*`, route => {
    reads++;
    return route.fulfill({json: {items: fleets, next_cursor: null, total: null}});
  });
  await page.goto(`${chatProbe}?address=1`);
  const input = page.getByRole("textbox", {name: "Message your fleet…"});
  await input.fill("Please check @");
  await input.pressSequentially("bo");
  const picker = page.getByRole("listbox", {name: "Workspace fleets"});
  await expect(picker.getByRole("option")).toHaveCount(1);
  await expect(picker.getByRole("option", {name: "@bob", exact: true})).toBeVisible();
  await page.screenshot({path: info.outputPath("workspace-fleet-picker.png"), animations: "disabled"});
  await input.press("Enter");
  await expect(input).toHaveValue("@bob Please check");
  expect(reads).toBe(1);
  await expect(picker).toHaveCount(0);
  await input.fill("@unknown");
  await expect(page.getByRole("status").filter({hasText: "No matching fleets."})).toBeVisible();
  await input.press("Escape");
  await expect(input).toHaveValue("@unknown");
  await expect(picker).toHaveCount(0);
  await input.fill("email bob@example");
  await expect(picker).toHaveCount(0);
  await input.fill("@");
  await expect(picker.getByRole("option")).toHaveCount(2);
  await input.press("ArrowDown");
  await input.press("Enter");
  await expect(input).toHaveValue("@chief-of-staff ");
});
