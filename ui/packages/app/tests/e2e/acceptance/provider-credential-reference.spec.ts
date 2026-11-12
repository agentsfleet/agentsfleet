/** A new provider key is stored in the selected workspace and registered as
 * the active account model through one Create model form. The inert key never
 * calls a provider. Cleanup removes only this workspace's exact fixture rows. */
import type { LibraryModel } from "@/lib/api/model-library-types";
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { gotoWorkspace } from "./fixtures/nav";
import { tenantModelPageSchema } from "@/lib/api/tenant-model-schemas";
import { modelLibrarySchema, providerLabel } from "@/lib/api/model-library-types";
import { OPENAI_COMPATIBLE_PROVIDER } from "@/lib/types";

const ACTION_TIMEOUT_MS = 15_000;
// The unique key name identifies only the credential and rows this journey owns.
const CREDENTIAL_NAME = `e2e-provider-${crypto.randomUUID()}`;
const INERT_PROVIDER_KEY = "e2e-inert-key-no-provider-access";
const MODELS = "/v1/tenants/me/models";
const MODEL_PAGE_MAX = 100;

async function resetProviderDirect(): Promise<void> {
  await clientFor(FIXTURE_KEY.regular).delete("/v1/tenants/me/provider").catch(() => undefined);
}

async function deleteCredentialDirect(name: string): Promise<void> {
  const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  await clientFor(FIXTURE_KEY.regular)
    .delete(`/v1/workspaces/${ws}/secrets/${encodeURIComponent(name)}`)
    .catch(() => undefined);
}

async function deleteFixtureModels(): Promise<void> {
  const client = clientFor(FIXTURE_KEY.regular);
  let cursor: string | null = null;
  const ids: string[] = [];
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  for (let page = 0; page < MODEL_PAGE_MAX; page++) {
    const query = new URLSearchParams({limit: "100"});
    if (cursor) query.set("starting_after", cursor);
    const result = tenantModelPageSchema.parse(await client.get<unknown>(`${MODELS}?${query}`));
    ids.push(...result.models.filter(model => model.workspace_id === workspace && model.secret_ref === CREDENTIAL_NAME).map(model => model.id));
    if (!result.next_cursor || result.next_cursor === cursor) break;
    cursor = result.next_cursor;
  }
  await Promise.all(ids.map(id => client.delete(`${MODELS}/${encodeURIComponent(id)}`)));
}

async function catalogueChoice(): Promise<LibraryModel> {
  const catalogue = modelLibrarySchema.parse(await clientFor(FIXTURE_KEY.regular).get("/v1/models"));
  // Match the dialog's initial ordinary catalogue page; never add or change models.
  const choice = catalogue.models.find((model) => !model.advanced && model.provider !== OPENAI_COMPATIBLE_PROVIDER);
  if (!choice) throw new Error("The fixture catalogue needs an ordinary provider model for the credential journey");
  return choice;
}

async function pickProvider(page: Page, provider: string): Promise<void> {
  const providerField = page.getByRole("dialog").getByLabel("Provider", { exact: true });
  await expect(providerField).toBeEnabled();
  const role = await providerField.getAttribute("role");
  if (role === "combobox") {
    await providerField.click();
    await page.getByRole("option", { name: providerLabel(provider), exact: true }).click({ timeout: 5_000 });
  } else {
    await providerField.fill(provider);
  }
}

// The catalogue-backed model picker is a <Select> when the catalogue covers
// the provider and degrades to a free-text <Input> when it doesn't — handle
// both so the spec survives either fixture-catalogue state.
async function pickModel(page: Page, choice: LibraryModel): Promise<void> {
  const model = page.getByRole("dialog").getByLabel("Model", { exact: true });
  await expect(model).toBeEnabled();
  const role = await model.getAttribute("role");
  if (role === "combobox") {
    await model.click();
    const label = choice.display_name === choice.id ? choice.id : `${choice.display_name} ${choice.id}`;
    await page.getByRole("option", { name: label, exact: true }).click({ timeout: 5_000 });
  } else {
    await model.fill(choice.id);
  }
}

test.describe("provider credential reference guard", () => {
  test.afterEach(async () => {
    await resetProviderDirect();
    await deleteFixtureModels();
    await deleteCredentialDirect(CREDENTIAL_NAME);
  });

  test("a provider key entered in the unified form becomes the active model credential", async ({ page }) => {
    await resetProviderDirect();
    await deleteCredentialDirect(CREDENTIAL_NAME);

    const choice = await catalogueChoice();
    await signInAs(page, FIXTURE_KEY.regular);
    const workspace = await gotoWorkspace(page, FIXTURE_KEY.regular, "settings/models");
    await expect(page.getByRole("heading", { name: /^models$/i })).toBeVisible();

    // Open the unified create-model dialog — one form, no tabs.
    await page.getByRole("button", { name: "Create model" }).click();
    const dialog = page.getByRole("dialog", {name: "Create model", exact: true});
    await expect(dialog).toBeVisible();
    await pickProvider(page, choice.provider);
    await pickModel(page, choice);
    const modelControl = dialog.getByRole("combobox", {name: "Model", exact: true});
    const selected = modelControl.getByTitle(choice.id, {exact: true});
    await expect(selected).toContainText(choice.id);
    const geometry = await modelControl.evaluate(node => {
      const control = node.getBoundingClientRect();
      const value = node.querySelector<HTMLElement>("[title]");
      const chevron = node.querySelector("svg")?.getBoundingClientRect();
      const box = value?.getBoundingClientRect();
      return {left: control.left, right: control.right, valueLeft: box?.left, valueRight: box?.right,
        chevronLeft: chevron?.left, chevronRight: chevron?.right,
        singleLine: value !== null && value.scrollHeight <= value.clientHeight + 1};
    });
    expect(geometry.singleLine).toBe(true);
    expect(geometry.valueLeft).toBeGreaterThanOrEqual(geometry.left - 1);
    expect(geometry.valueRight).toBeLessThanOrEqual(geometry.chevronLeft ?? 0);
    expect(geometry.chevronRight).toBeLessThanOrEqual(geometry.right + 1);
    const addKey = dialog.getByRole("button", {name: "Add new key", exact: true});
    await addKey.click();
    await expect(addKey).toHaveAttribute("aria-pressed", "true");
    await dialog.getByLabel("Key name", {exact: true}).fill(CREDENTIAL_NAME);
    await dialog.getByLabel("API key", {exact: true}).fill(INERT_PROVIDER_KEY);
    const active = dialog.getByRole("switch", {name: "Make active", exact: true});
    await active.check();
    await expect(active).toBeChecked();
    await page.screenshot({path: test.info().outputPath("live-create-model.png"), scale: "css", animations: "disabled"});
    await dialog.getByRole("button", {name: "Create model", exact: true}).click();

    // Registration and activation do not call the model provider. The inert
    // key must produce our exact stored reference and active model entry.
    await expect(dialog).not.toBeVisible({ timeout: ACTION_TIMEOUT_MS });
    await expect.poll(async () => {
      const result = tenantModelPageSchema.parse(await clientFor(FIXTURE_KEY.regular).get(`${MODELS}?limit=100`));
      return result.models.find((entry) => entry.workspace_id === workspace && entry.secret_ref === CREDENTIAL_NAME);
    }, { timeout: ACTION_TIMEOUT_MS }).toMatchObject({
      workspace_id: workspace,
      provider: choice.provider,
      model_id: choice.id,
      secret_ref: CREDENTIAL_NAME,
      has_key: true,
      active: true,
    });
    const activeRow = page.getByRole("region", {name: "Models", exact: true}).getByRole("article").filter({
      has: page.getByRole("button", { name: `View details for ${choice.id}`, exact: true }),
    }).filter({ has: page.getByText("Active", { exact: true }) }).filter({hasText: CREDENTIAL_NAME});
    await expect(activeRow).toHaveCount(1);
    await expect(activeRow).toContainText(providerLabel(choice.provider));
    await expect(activeRow).toContainText(choice.display_name);

  });
});
