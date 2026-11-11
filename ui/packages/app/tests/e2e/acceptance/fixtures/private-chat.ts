/**
 * private-chat.ts — the private fleet-setup chat as one person sees it.
 *
 * Every locator takes the page of the person whose chat it is. The absence
 * check reads the whole document, scripts and form values included, because a
 * private chat leaks through a hydration payload or a prefilled field as easily
 * as through visible text.
 */
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { workspaceHref } from "./nav";
import type { ClientHandle } from "./api-client";
import { readFixtureFleet } from "./native-setup";

export const NEW_FLEET = "fleets/new";
const SAVE_TIMEOUT_MS = 60_000;
const FLEET_URL = /\/w\/[^/]+\/fleets\/(?!new)[a-z0-9-]+(\?|$)/;

/** The words the shipped chat shows, held here so a spec pins what a person reads. */
export const CHAT_COPY = {
  setup: "Fleet setup",
  editSetup: "Instructions",
  instructions: "Instructions",
  updateInstructions: "Save changes",
  save: "Save fleet",
  apply: "Save changes",
  openFleet: "Open fleet",
  improve: "Edit setup",
  composer: "Setup chat composer",
  send: "Send message",
  unavailable: "Chat is unavailable",
  historyUnavailable: "Unfinished setups could not be loaded.",
} as const;

export function draftHref(workspaceId: string, draftId: string): string {
  return `${workspaceHref(workspaceId, NEW_FLEET)}?draft=${encodeURIComponent(draftId)}`;
}

/** The private chat's entry under Unfinished setups on New fleet. */
export function historyEntry(page: Page, draftId: string): Locator {
  return page.locator(`a[href$="?draft=${encodeURIComponent(draftId)}"]`);
}

export async function setupAction(page: Page, name: string): Promise<Locator> {
  await page.getByRole("button", {name: "Setup chat actions", exact: true}).click();
  return page.getByRole("menuitem", {name, exact: true});
}

/** Exercise the runtime header menu rather than bypassing it through the footer. */
export async function editSetupAction(page: Page): Promise<Locator> {
  await page.getByRole("button", {name: "Fleet actions", exact: true}).click();
  return page.getByRole("menuitem", {name: CHAT_COPY.improve, exact: true});
}

export async function expectEditSetupAvailable(page: Page): Promise<void> {
  await expect(await editSetupAction(page)).toBeVisible();
  await page.keyboard.press("Escape");
  const trigger = page.getByRole("button", {name: "Fleet actions", exact: true});
  await expect(trigger).toHaveAttribute("aria-expanded", "false");
  await expect(trigger).toBeFocused();
}

export async function saveSetupButton(page: Page): Promise<Locator> {
  return page.getByRole("button", {name: CHAT_COPY.save, exact: true});
}

/** A confirmed save navigates to the same workspace's durable fleet and revision. */
export async function openSavedFleet(page: Page, {handle, workspaceId, revision = "1"}: {
  handle: ClientHandle; workspaceId: string; revision?: string;
}): Promise<string> {
  await expect(page).toHaveURL(FLEET_URL, {timeout: SAVE_TIMEOUT_MS});
  const id = new URL(page.url()).pathname.split("/").at(-1);
  if (!id) throw new Error("Saving returned no fleet identity");
  await expect(page).toHaveURL(workspaceHref(workspaceId, `fleets/${id}`));
  await expect.poll(async () => (await readFixtureFleet(handle, workspaceId, id)).setup_revision,
    {timeout: SAVE_TIMEOUT_MS}).toBe(revision);
  await expect(page.getByRole("form", {name: "Chat composer", exact: true}).getByRole("textbox")).toBeEnabled();
  return id;
}

export function setupSummary(page: Page): Locator {
  return page.getByRole("region", {name: CHAT_COPY.setup, exact: true});
}

export function chatUnavailable(page: Page): Locator {
  return page.getByRole("alert").filter({ hasText: CHAT_COPY.unavailable });
}

/** Opens a chat by the address its history entry links to and waits for its candidate. */
export async function openDraft(page: Page, workspaceId: string, draftId: string): Promise<void> {
  await page.goto(draftHref(workspaceId, draftId));
  await expect(setupSummary(page)).toBeVisible();
}

/** Sends `text` from the composer with the keyboard, the way a person would. */
export async function sendFromComposer(page: Page, text: string): Promise<void> {
  const input = page.getByLabel(CHAT_COPY.composer).getByRole("textbox");
  await input.fill(text);
  await input.press("Enter");
}

/** No byte of the document — text, scripts or form values — carries `text`. */
export async function expectPrivateTextAbsent(page: Page, text: string, state: string): Promise<void> {
  const html = await page.content();
  const fields = await page.locator("input, textarea").evaluateAll((nodes) =>
    nodes.map((node) => node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement ? node.value : ""));
  // Booleans only: a failure must never print another subject's private text into the report.
  expect(html.includes(text), `${state}: private text in the document`).toBe(false);
  expect(fields.some((value) => value.includes(text)), `${state}: private text in a form field`).toBe(false);
}
