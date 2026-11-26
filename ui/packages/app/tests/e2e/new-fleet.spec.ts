import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { NEW_FLEET } from "./fixtures/new-fleet-data";
import { FLEET_DETAIL, FLEET_ID, MODEL, WORKSPACE } from "@/tests/helpers/fleet-fixtures";

// The probe mounts New fleet and the fleet page under a router that moves the
// address bar; its server stands in for `POST W/fleets` and the new fleet's thread read.
const CREATE_PATH = `/live/v1/workspaces/${WORKSPACE}/fleets`;
const MESSAGE = "Triage the newest support tickets and draft replies for the duplicates.";

test("a first message opens the fleet's chat", async ({page, chatProbe, isMobile}) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  const creates: unknown[] = [];
  page.on("request", request => {
    if (request.method() === "POST" && new URL(request.url()).pathname === CREATE_PATH) creates.push(request.postDataJSON());
  });
  const forbidden: string[] = [];
  page.on("request", request => {if (/\/(?:drafts|setup-context|revisions)\b/.test(request.url())) forbidden.push(request.url());});
  await page.goto(`${chatProbe}?newFleet=1`);
  const composer = page.getByRole("form", {name: "New fleet composer"});
  await expect(composer.getByRole("combobox", {name: "Fleet model"})).toHaveText(MODEL.label);
  const input = composer.getByRole("textbox", {name: "What would you like your fleet to do?"});
  await input.fill(MESSAGE);
  await input.press("Enter");

  await expect(page).toHaveURL(new RegExp(`/w/${WORKSPACE}/fleets/${NEW_FLEET.id}\\?newFleet=1$`));
  expect(creates).toHaveLength(1);
  expect(creates[0]).toEqual({message: MESSAGE, model_ref: MODEL.ref, operation_id: expect.stringMatching(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-/)});
  const chat = page.getByRole("log", {name: "Chat"});
  await expect(chat.getByText(MESSAGE, {exact: true})).toBeVisible();
  await expect(chat.getByText(NEW_FLEET.answer, {exact: true})).toBeVisible();
  await expect(page.getByRole("heading", {name: NEW_FLEET.name, exact: true})).toBeVisible();

  if (isMobile) {
    await page.setViewportSize({width: 320, height: 840});
    const model = page.getByRole("combobox", {name: "Fleet model"});
    await expect(model).toHaveText("Runtime model");
    expect(await model.getByText("Runtime model", {exact: true}).evaluate(label => label.scrollWidth <= label.clientWidth)).toBe(true);
    await model.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("option", {name: "Runtime model"})).toBeVisible();
    await page.keyboard.press("Escape");
  }
  if (isMobile) await page.getByRole("button", {name: "Open navigation", exact: true}).click();
  const nav = page.getByRole("navigation", {name: isMobile ? "Expanded navigation" : "Primary"});
  await expect(nav.getByRole("link", {name: NEW_FLEET.name, exact: true})).toHaveAttribute("href", `/w/${WORKSPACE}/fleets/${NEW_FLEET.id}`);
  expect(creates).toHaveLength(1);
  expect(forbidden).toEqual([]);
  expect(errors).toEqual([]);
});

test("a rename refreshes the fleet label and sidebar, and saved memories preserve an unsent message", async ({page, chatProbe, isMobile}) => {
  const resource = `${CREATE_PATH}/${NEW_FLEET.id}`;
  const renamed = "support-review";
  let name: string = NEW_FLEET.name;
  const mutations: unknown[] = [];
  await page.route(`**${resource}`, async route => {
    const request = route.request();
    mutations.push({method: request.method(), tag: request.headers()["if-match"], body: request.postDataJSON()});
    name = renamed;
    await route.fulfill({json: {...FLEET_DETAIL, id: NEW_FLEET.id, name, etag: '"renamed"'}});
  });
  await page.route(url => url.pathname === CREATE_PATH && url.searchParams.has("limit"), route =>
    route.fulfill({json: {items: [{...FLEET_DETAIL, id: NEW_FLEET.id, name}], total: null, next_cursor: null}}));
  const requested = Promise.withResolvers<void>();
  await page.route(url => url.pathname === `${resource}/memories` && url.search === "", async route => {
    await requested.promise;
    const memory = {category: "core", updated_at: FLEET_DETAIL.updated_at};
    await route.fulfill({json: {items: [
      {...memory, key: "support-queue", content: "Use the support queue.", visibility: "fleet", writer_fleet_id: NEW_FLEET.id},
      {...memory, key: "release-notes", content: "Check the release notes.", visibility: "workspace", writer_fleet_id: FLEET_ID},
    ]}});
  });

  await page.goto(`${chatProbe}?newFleet=1`);
  const firstMessage = page.getByRole("textbox", {name: "What would you like your fleet to do?"});
  await firstMessage.fill(MESSAGE);
  await firstMessage.press("Enter");
  await expect(page.getByRole("heading", {name: NEW_FLEET.name, exact: true})).toBeVisible();
  if (isMobile) await page.setViewportSize({width: 320, height: 840});
  const composer = page.getByRole("textbox");
  await composer.fill("Keep this draft.");
  const actions = page.getByRole("button", {name: "Fleet actions", exact: true});
  await actions.click();
  await page.getByRole("menuitem", {name: "Rename fleet"}).click();
  const editor = page.getByRole("textbox", {name: "Fleet name", exact: true});
  await expect(editor).toBeFocused();
  await editor.fill("Support Review");
  await editor.press("Enter");
  await expect(page.getByRole("heading", {name: renamed, exact: true})).toBeVisible();
  await expect(page.getByRole("log", {name: "Chat"}).getByText(renamed, {exact: true})).toBeVisible();
  expect(mutations).toEqual([{method: "PATCH", tag: FLEET_DETAIL.etag, body: {name: renamed}}]);

  await actions.click();
  const menu = page.getByRole("menu");
  for (const description of ["Use memories from other fleets.", "Let other fleets use what it shares."]) {
    const lines = await menu.getByText(description).evaluate(element => element.clientHeight / parseFloat(getComputedStyle(element).lineHeight));
    expect(lines).toBeLessThan(1.1);
  }
  await menu.getByRole("menuitem", {name: "Saved memories"}).click();
  const memories = page.getByRole("dialog", {name: "Saved memories"});
  await expect(memories.getByRole("status").locator("svg")).toBeVisible();
  requested.resolve();
  await expect(memories.getByRole("region", {name: "Saved by this fleet"}).getByText("Use the support queue.")).toBeVisible();
  const shared = memories.getByRole("region", {name: "Shared by other fleets"});
  await expect(shared.getByText("Check the release notes.")).toBeVisible();
  await expect(shared.getByRole("button")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(actions).toBeFocused();
  await expect(composer).toHaveValue("Keep this draft.");
  if (isMobile) await page.getByRole("button", {name: "Open navigation", exact: true}).click();
  const nav = page.getByRole("navigation", {name: isMobile ? "Expanded navigation" : "Primary"});
  await expect(nav.getByRole("link", {name: renamed, exact: true})).toHaveAttribute("href", `/w/${WORKSPACE}/fleets/${NEW_FLEET.id}`);
});
