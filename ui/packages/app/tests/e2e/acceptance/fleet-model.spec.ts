import { expect, test } from "@playwright/test";
import { z } from "zod";
import { ModelsSchema, type WorkspaceModel } from "@/lib/api/draft-types";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { EDIT_SETUP_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { withReplyPage } from "./fixtures/reply-page";

const MODELS = "/v1/tenants/me/models";
const PLATFORM_DEFAULT = "platform-default";
const ENTRY = z.object({id: z.uuid()});
const MESSAGE = "Keep my unsent question after the model changes";
const SETUP_MESSAGE = "Keep my unsent setup edit too";
const MODEL_LABEL = "Fleet model";
const EDITOR_LABEL = "Edit fleet setup";

test("saving the fleet model preserves both composers and survives reload", async ({page}) => {
  await withRuntimeChoice(async choice => {
    await withReplyPage(page, "incident-routing", async ({workspaceId, fleetId}) => {
      const composer = page.getByLabel("Chat composer", {exact: true}).getByRole("textbox");
      const opener = page.getByRole("button", {name: EDITOR_LABEL, exact: true});
      await composer.fill(MESSAGE);
      await opener.click();
      const setup = page.getByRole("complementary", {name: EDITOR_LABEL});
      const setupInput = setup.getByLabel(EDIT_SETUP_PLACEHOLDER);
      await expect(setupInput).toBeEnabled();
      await setupInput.fill(SETUP_MESSAGE);
      await setup.getByRole("button", {name: "Close setup editor"}).click();
      const postedMessages: string[] = [];
      page.on("request", request => {
        if (request.method() === "POST" && new URL(request.url()).pathname.endsWith("/messages")) postedMessages.push(request.url());
      });
      await page.getByRole("combobox", {name: MODEL_LABEL}).click();
      await page.getByRole("option", {name: choice.label, exact: true}).and(page.locator('[data-state="unchecked"]')).first().click();
      await expect.poll(async () => (await readFleet(workspaceId, fleetId)).setup_revision).toBe("2");
      await expect(page.getByRole("combobox", {name: MODEL_LABEL})).toContainText(choice.label);
      expect((await readFleet(workspaceId, fleetId)).definition.config["x-agentsfleet"].model).toBe(choice.ref);
      await expect(composer).toHaveValue(MESSAGE);
      await opener.click();
      await expect(setupInput).toHaveValue(SETUP_MESSAGE);
      await setup.getByRole("button", {name: "Close setup editor"}).click();
      expect(postedMessages).toEqual([]);
      await page.reload();
      await expect(page.getByRole("combobox", {name: MODEL_LABEL})).toContainText(choice.label);
    });
  });
});

async function readFleet(workspace: string, fleet: string) {
  return FleetDetailSchema.parse(await clientFor(FIXTURE_KEY.regular).get<unknown>(`/v1/workspaces/${workspace}/fleets/${fleet}`));
}

/** A dummy credential allows selection without sending any inference request. */
async function withRuntimeChoice(body: (choice: WorkspaceModel) => Promise<void>): Promise<void> {
  const client = clientFor(FIXTURE_KEY.regular);
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const modelPath = `/v1/workspaces/${workspace}/models`;
  const choices = ModelsSchema.parse(await client.get<unknown>(modelPath));
  const existing = choices.items.find(model => model.ref !== PLATFORM_DEFAULT && model.eligibility.runtime);
  if (existing) {await body(existing); return;}
  const platform = choices.items.find(model => model.ref === PLATFORM_DEFAULT && model.eligibility.runtime);
  if (!platform) throw new Error("No priced runtime model available for the fixture");
  const secret = `model-selection-${crypto.randomUUID()}`;
  const secrets = `/v1/workspaces/${workspace}/secrets`;
  await client.post(secrets, {name: secret, data: {provider: platform.provider, api_key: crypto.randomUUID()}});
  let id: string | undefined;
  try {
    const created = ENTRY.parse(await client.post<unknown>(MODELS, {model_id: platform.model, secret_ref: secret}));
    id = created.id;
    const model = ModelsSchema.parse(await client.get<unknown>(modelPath)).items.find(item => item.ref === id);
    if (!model?.eligibility.runtime) throw new Error("Fixture model is not runtime eligible");
    await body(model);
  } finally {
    try {if (id) await client.delete(`${MODELS}/${id}`);}
    finally {await client.delete(`${secrets}/${secret}`);}
  }
}
