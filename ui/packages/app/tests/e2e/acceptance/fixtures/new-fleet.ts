/**
 * new-fleet.ts — New fleet as a person uses it: type a first message, and the
 * page becomes the chat of the fleet it made.
 */
import { expect, type Page } from "@playwright/test";
import { NEW_FLEET_COPY } from "./constants";
import { workspaceHref, workspaceUrlPattern } from "./nav";

const NEW_FLEET = "fleets/new";
const FLEET_URL = /\/w\/[^/]+\/fleets\/(?!new)[a-z0-9-]+(\?|$)/;
const CREATE_TIMEOUT_MS = 60_000;

/**
 * Types a first message on New fleet the way a person does and follows the
 * page to the fleet it made. The fleet is named by the daemon, so the caller
 * owns it by the identity this returns, never by a name prefix.
 */
export async function createFleetViaUI(page: Page, workspaceId: string, message: string): Promise<string> {
  await page.goto(workspaceHref(workspaceId, NEW_FLEET));
  await expect(page).toHaveURL(workspaceUrlPattern(NEW_FLEET));
  const composer = page.getByRole("form", {name: NEW_FLEET_COPY.composer, exact: true});
  await composer.getByPlaceholder(NEW_FLEET_COPY.placeholder).fill(message);
  await composer.getByRole("button", {name: NEW_FLEET_COPY.send, exact: true}).click();
  await expect(page).toHaveURL(FLEET_URL, {timeout: CREATE_TIMEOUT_MS});
  const id = new URL(page.url()).pathname.split("/").at(-1);
  if (!id) throw new Error("the first message opened no fleet");
  await expect(page).toHaveURL(workspaceHref(workspaceId, `fleets/${id}`));
  await expect(page.getByRole("form", {name: "Chat composer", exact: true}).getByRole("textbox")).toBeEnabled({timeout: CREATE_TIMEOUT_MS});
  return id;
}
