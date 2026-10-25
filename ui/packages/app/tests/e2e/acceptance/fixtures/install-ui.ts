import type { Definition } from "@/lib/api/draft-types";
import type { ClientHandle } from "./api-client";
import { expect, type Page } from "@playwright/test";
import { fixtureConfig, fixtureDefinition, executionInstructions } from "./definition";
import { createFixtureDraft } from "./native-setup";
import { workspaceHref, workspaceUrlPattern } from "./nav";
import { CHAT_COPY, setupAction } from "./private-chat";

const SAVE_TIMEOUT_MS = 60_000;
const NEW_FLEET = "fleets/new";

export interface SaveAuth {
  handle: ClientHandle;
  workspaceId: string;
  instructions?: string;
  config?: Definition["config"];
}

/** Edit the private chat's instructions, then save explicitly. */
export async function saveFleetViaUI(page: Page, name: string, auth: SaveAuth): Promise<string> {
  const draft = await createFixtureDraft(auth.handle, auth.workspaceId, fixtureDefinition(name));
  await page.goto(`${workspaceHref(auth.workspaceId, NEW_FLEET)}?draft=${encodeURIComponent(draft.id)}`);
  await expect(page).toHaveURL(workspaceUrlPattern(NEW_FLEET));
  await (await setupAction(page, CHAT_COPY.editSetup)).click();
  const editor = page.getByRole("dialog", {name: CHAT_COPY.editSetup, exact: true});
  await editor.getByLabel("Fleet name", { exact: true }).fill(name);
  await editor.getByLabel(CHAT_COPY.instructions, { exact: true }).fill(auth.instructions ?? executionInstructions());
  await editor.getByRole("button", {name: "Advanced settings", exact: true}).click();
  await editor.getByLabel("Runtime config", { exact: true }).fill(JSON.stringify(auth.config ?? fixtureConfig(name)));
  await editor.getByRole("button", { name: CHAT_COPY.updateInstructions }).click();
  // Save validates the displayed candidate before creating a fleet.
  await page.getByRole("button", { name: CHAT_COPY.save, exact: true }).click();
  await expect(page).toHaveURL(/\/w\/[^/]+\/fleets\/(?!new)[a-z0-9-]+(\?|$)/, { timeout: SAVE_TIMEOUT_MS });
  const id = new URL(page.url()).pathname.split("/").pop();
  if (!id) throw new Error("Save returned no fleet identity");
  return id;
}
