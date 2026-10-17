import type { Definition } from "@/lib/api/draft-types";
import type { ClientHandle } from "./api-client";
import { expect, type Page } from "@playwright/test";
import { fixtureConfig, fixtureDefinition, executionInstructions } from "./definition";
import { createFixtureDraft } from "./native-setup";
import { workspaceHref, workspaceUrlPattern } from "./nav";

const SAVE_TIMEOUT_MS = 60_000;
const NEW_FLEET = "fleets/new";

export interface SaveAuth {
  handle: ClientHandle;
  workspaceId: string;
  instructions?: string;
  config?: Definition["config"];
}

/**
 * Review and edit a private chat that already holds a candidate, then Save explicitly.
 *
 * Review setup renders only once a chat has a candidate. An authoring-model reply is
 * not deterministic, so the candidate comes from a definition-source draft created as
 * the signed-in person; the browser then opens it through the address the sidebar's
 * "Your chats" entry links to and does everything else through the shipped dialog.
 */
export async function saveFleetViaUI(page: Page, name: string, auth: SaveAuth): Promise<string> {
  const draft = await createFixtureDraft(auth.handle, auth.workspaceId, fixtureDefinition(name));
  await page.goto(`${workspaceHref(auth.workspaceId, NEW_FLEET)}?draft=${encodeURIComponent(draft.id)}`);
  await expect(page).toHaveURL(workspaceUrlPattern(NEW_FLEET));
  await page.getByRole("button", { name: "Review setup", exact: true }).click();
  const review = page.getByRole("dialog", { name: "Review fleet setup" });
  await review.getByRole("button", { name: "Edit setup", exact: true }).click();
  await review.getByLabel("Instance name", { exact: true }).fill(name);
  await review.getByLabel("Instructions", { exact: true }).fill(auth.instructions ?? executionInstructions());
  await review.getByLabel("Runtime config", { exact: true }).fill(JSON.stringify(auth.config ?? fixtureConfig(name)));
  await review.getByRole("button", { name: "Update draft and review" }).click();
  // Review is fetched after the editor mutation, so Save always binds the displayed candidate.
  await review.getByRole("button", { name: "Save fleet", exact: true }).click();
  await review.getByRole("link", { name: /open fleet/i }).click({ timeout: SAVE_TIMEOUT_MS });
  await expect(page).toHaveURL(/\/w\/[^/]+\/fleets\/(?!new)[a-z0-9-]+(\?|$)/, { timeout: SAVE_TIMEOUT_MS });
  const id = new URL(page.url()).pathname.split("/").pop();
  if (!id) throw new Error("Save returned no fleet identity");
  return id;
}
