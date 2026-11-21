import type { Definition } from "@/lib/api/draft-types";
import type { ClientHandle } from "./api-client";
import { expect, type Page } from "@playwright/test";
import { fixtureConfig, fixtureDefinition, executionInstructions } from "./definition";
import { createFixtureDraft } from "./native-setup";
import { workspaceHref, workspaceUrlPattern } from "./nav";
import { privateSaveButton, openSavedFleet } from "./private-chat";
const NEW_FLEET = "fleets/new";

export interface SaveAuth {
  handle: ClientHandle;
  workspaceId: string;
  instructions?: string;
  config?: Definition["config"];
}

/** Save the reviewed instructions and follow the receipt into the runtime conversation. */
export async function saveFleetViaUI(page: Page, name: string, auth: SaveAuth): Promise<string> {
  const draft = await createFixtureDraft(auth.handle, auth.workspaceId,
    fixtureDefinition(name, auth.instructions ?? executionInstructions(), auth.config ?? fixtureConfig(name)));
  await page.goto(`${workspaceHref(auth.workspaceId, NEW_FLEET)}?draft=${encodeURIComponent(draft.id)}`);
  await expect(page).toHaveURL(workspaceUrlPattern(NEW_FLEET));
  await (await privateSaveButton(page)).click();
  return openSavedFleet(page, auth);
}
