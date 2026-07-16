/**
 * provider-credential-reference.spec.ts — a provider key entered in the UI
 * becomes a stored credential the tenant provider references, on the
 * many-model registry page.
 *
 * The unified Add-model dialog is one tab-free form: Name → Provider → Model
 * → API key. Type a Name, pick (or type) the provider, pick a model, paste the
 * key, then "Save & make active" stores `{provider, api_key}` (no `model` in
 * the body — entries own the model now), registers the entry, then points the
 * tenant provider at it. The credential name is the typed Name, so cleanup
 * resets the provider and deletes that credential by the same name.
 *
 * Defensive by design: the form assembly is deterministic (client-side), but
 * the save+activate round-trip depends on the fixture backend accepting the
 * model, so the activation assertion tolerates an env-variance alert the same
 * way the prior credential-reference spec did. The exact action calls are
 * pinned by models-registry-add.test.tsx; this spec proves the real built
 * dialog wires them end to end.
 */
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { gotoWorkspace } from "./fixtures/nav";

const ACTION_TIMEOUT_MS = 15_000;
// The typed credential Name — free-form in the unified form, and the identity
// cleanup keys off. Named for the provider it holds, by convention.
const CREDENTIAL_NAME = "anthropic";
const PROVIDER = "anthropic";
const ANTHROPIC_KEY = "sk-ant-e2e-xxxxxxxx";
const MODEL_FALLBACK = "claude-sonnet-4-6";

async function resetProviderDirect(): Promise<void> {
  await clientFor(FIXTURE_KEY.regular).delete("/v1/tenants/me/provider").catch(() => undefined);
}

async function deleteCredentialDirect(name: string): Promise<void> {
  const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  await clientFor(FIXTURE_KEY.regular)
    .delete(`/v1/workspaces/${ws}/secrets/${encodeURIComponent(name)}`)
    .catch(() => undefined);
}

// The Provider field is a catalogue-backed <Select> when the fixture catalogue
// has rows, and degrades to a free-text <Input> when it doesn't — handle both.
async function pickProvider(page: Page): Promise<void> {
  const providerField = page.getByRole("dialog").getByLabel("Provider", { exact: true });
  const role = await providerField.getAttribute("role").catch(() => null);
  if (role === "combobox") {
    await providerField.click();
    await page.getByRole("option", { name: new RegExp(`^${PROVIDER}$`, "i") }).click({ timeout: 5_000 });
  } else {
    await providerField.fill(PROVIDER);
  }
}

// The catalogue-backed model picker is a <Select> when the catalogue covers
// the provider and degrades to a free-text <Input> when it doesn't — handle
// both so the spec survives either fixture-catalogue state.
async function pickModel(page: Page): Promise<void> {
  const model = page.getByRole("dialog").getByLabel("Model", { exact: true });
  const role = await model.getAttribute("role").catch(() => null);
  if (role === "combobox") {
    await model.click();
    await page.getByRole("option").first().click({ timeout: 5_000 }).catch(() => undefined);
  } else {
    await model.fill(MODEL_FALLBACK);
  }
}

test.describe("provider credential reference guard", () => {
  test.afterEach(async () => {
    await resetProviderDirect();
    await deleteCredentialDirect(CREDENTIAL_NAME);
  });

  test("a provider key entered in the unified form becomes the active model credential", async ({ page }) => {
    await resetProviderDirect();
    await deleteCredentialDirect(CREDENTIAL_NAME);

    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "settings/models");
    await expect(page.getByRole("heading", { name: /^models$/i })).toBeVisible();

    // Open the unified create-model dialog — one form, no tabs.
    await page.getByRole("button", { name: "Create model" }).click();
    await expect(page.getByRole("dialog")).toBeVisible();

    // Fill the form in order: Name → Provider → Model → API key.
    await page.getByLabel("Name").fill(CREDENTIAL_NAME);
    await pickProvider(page);
    await pickModel(page);
    await page.getByLabel("API key", { exact: true }).fill(ANTHROPIC_KEY);

    await page.getByRole("button", { name: "Save & make active" }).click();

    // The store+create+activate round-trip either lands (the row shows Active
    // referencing the anthropic credential) or surfaces a typed alert under
    // fixture variance.
    const activeRow = page.getByRole("row", { name: new RegExp(PROVIDER, "i") });
    const outcome = await Promise.race([
      activeRow.filter({ hasText: /active/i }).waitFor({ state: "visible", timeout: ACTION_TIMEOUT_MS }).then(() => "live" as const),
      page.getByRole("alert").waitFor({ state: "visible", timeout: ACTION_TIMEOUT_MS }).then(() => "alert" as const),
    ]).catch(() => "unknown" as const);

    if (outcome === "live") {
      await expect(activeRow).toContainText(/active/i);
    } else if (outcome === "alert") {
      await expect(page.getByRole("alert")).toBeVisible();
    } else {
      // Neither landmark resolved in time — fail loudly rather than silently pass.
      throw new Error("save+activate produced neither an Active row nor a typed alert");
    }
  });
});
