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

/** Edit a private chat's inline setup, then save explicitly. */
export async function saveFleetViaUI(page: Page, name: string, auth: SaveAuth): Promise<string> {
  const draft = await createFixtureDraft(auth.handle, auth.workspaceId, fixtureDefinition(name));
  await page.goto(`${workspaceHref(auth.workspaceId, NEW_FLEET)}?draft=${encodeURIComponent(draft.id)}`);
  await expect(page).toHaveURL(workspaceUrlPattern(NEW_FLEET));
  const setup = page.getByRole("region", {name: "Fleet setup", exact: true});
  await setup.getByRole("button", { name: "Edit instructions", exact: true }).click();
  await setup.getByLabel("Instance name", { exact: true }).fill(name);
  await setup.getByLabel("Instructions", { exact: true }).fill(auth.instructions ?? executionInstructions());
  await setup.getByLabel("Runtime config", { exact: true }).fill(JSON.stringify(auth.config ?? fixtureConfig(name)));
  await setup.getByRole("button", { name: "Update instructions" }).click();
  // Save validates the displayed candidate before creating a fleet.
  await setup.getByRole("button", { name: "Save fleet", exact: true }).click();
  await page.getByRole("link", { name: /open fleet/i }).click({ timeout: SAVE_TIMEOUT_MS });
  await expect(page).toHaveURL(/\/w\/[^/]+\/fleets\/(?!new)[a-z0-9-]+(\?|$)/, { timeout: SAVE_TIMEOUT_MS });
  const id = new URL(page.url()).pathname.split("/").pop();
  if (!id) throw new Error("Save returned no fleet identity");
  return id;
}
