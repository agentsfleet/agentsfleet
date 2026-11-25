import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { NEW_FLEET } from "./fixtures/new-fleet-data";
import { WORKSPACE } from "@/tests/helpers/fleet-fixtures";

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
  const input = composer.getByRole("textbox", {name: "What would you like your fleet to do?"});
  await input.fill(MESSAGE);
  await input.press("Enter");

  await expect(page).toHaveURL(new RegExp(`/w/${WORKSPACE}/fleets/${NEW_FLEET.id}\\?newFleet=1$`));
  expect(creates).toHaveLength(1);
  expect(creates[0]).toEqual({message: MESSAGE, operation_id: expect.stringMatching(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-/)});
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
