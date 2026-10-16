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

export const NEW_FLEET = "fleets/new";

/** The words the shipped chat shows, held here so a spec pins what a person reads. */
export const CHAT_COPY = {
  review: "Review setup",
  reviewDialog: "Review fleet setup",
  editSetup: "Edit setup",
  instructions: "Instructions",
  updateReview: "Update draft and review",
  save: "Save fleet",
  apply: "Apply reviewed setup",
  applied: "Fleet setup updated. New work uses this setup revision.",
  savedSetup: "Saved fleet setup",
  openFleet: "Open fleet",
  improve: "Improve in private chat",
  composer: "Private chat composer",
  composerInput: "Describe the work you want this fleet to do…",
  send: "Send message",
  unavailable: "Chat is unavailable",
  historyUnavailable: "Chat history is unavailable.",
} as const;

export function draftHref(workspaceId: string, draftId: string): string {
  return `${workspaceHref(workspaceId, NEW_FLEET)}?draft=${encodeURIComponent(draftId)}`;
}

/** The chat's entry under "Fleet setup chats" in the sidebar. */
export function historyEntry(page: Page, draftId: string): Locator {
  return page.locator(`a[href$="?draft=${encodeURIComponent(draftId)}"]`);
}

export function reviewButton(page: Page): Locator {
  return page.getByRole("button", { name: CHAT_COPY.review, exact: true });
}

export function reviewDialog(page: Page): Locator {
  return page.getByRole("dialog", { name: CHAT_COPY.reviewDialog });
}

/** The saved-fleet result. The chat and the open review dialog each render one, so callers scope it. */
export function savedSetup(scope: Page | Locator): Locator {
  return scope.getByLabel(CHAT_COPY.savedSetup, { exact: true });
}

export function chatUnavailable(page: Page): Locator {
  return page.getByRole("alert").filter({ hasText: CHAT_COPY.unavailable });
}

/** Opens a chat by the address its history entry links to and waits for its candidate. */
export async function openDraft(page: Page, workspaceId: string, draftId: string): Promise<void> {
  await page.goto(draftHref(workspaceId, draftId));
  await expect(reviewButton(page)).toBeVisible();
}

/** Sends `text` from the composer with the keyboard, the way a person would. */
export async function sendFromComposer(page: Page, text: string): Promise<void> {
  const input = page.getByLabel(CHAT_COPY.composer).getByRole("textbox", { name: CHAT_COPY.composerInput });
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
