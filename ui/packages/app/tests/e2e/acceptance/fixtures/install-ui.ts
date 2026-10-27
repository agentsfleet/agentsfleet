import type { Definition } from "@/lib/api/draft-types";
import type { ClientHandle } from "./api-client";
import { expect, type Page } from "@playwright/test";
import { fixtureConfig, fixtureDefinition, executionInstructions } from "./definition";
import { createFixtureDraft } from "./native-setup";
import { workspaceHref, workspaceUrlPattern } from "./nav";
import { CHAT_COPY, openSavedFleet, setupAction } from "./private-chat";
const NEW_FLEET = "fleets/new";

export interface SaveAuth {
  handle: ClientHandle;
  workspaceId: string;
  instructions?: string;
  config?: Definition["config"];
}

/** Edit instructions, save in authoring, then explicitly open the saved fleet. */
export async function saveFleetViaUI(page: Page, name: string, auth: SaveAuth): Promise<string> {
  const draft = await createFixtureDraft(auth.handle, auth.workspaceId,
    fixtureDefinition(name, undefined, auth.config ?? fixtureConfig(name)));
  await page.goto(`${workspaceHref(auth.workspaceId, NEW_FLEET)}?draft=${encodeURIComponent(draft.id)}`);
  await expect(page).toHaveURL(workspaceUrlPattern(NEW_FLEET));
  await (await setupAction(page, CHAT_COPY.editSetup)).click();
  const editor = page.getByRole("dialog", {name: CHAT_COPY.editSetup, exact: true});
  await editor.getByLabel("Fleet name", { exact: true }).fill(name);
  await editor.getByLabel(CHAT_COPY.instructions, { exact: true }).fill(auth.instructions ?? executionInstructions());
  await editor.getByRole("button", { name: CHAT_COPY.updateInstructions }).click();
  await expect(editor).toBeHidden();
  await page.getByRole("button", { name: CHAT_COPY.save, exact: true }).click();
  return openSavedFleet(page);
}
