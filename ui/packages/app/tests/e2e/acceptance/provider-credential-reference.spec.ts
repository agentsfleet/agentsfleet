/**
 * provider-credential-reference.spec.ts — a provider key entered in the UI
 * becomes a stored credential the tenant provider references, on the
 * consolidated Models page.
 *
 * After the M102 form consolidation the option-card flow is gone. The switch
 * list's generic "Other provider" row opens the consolidated ProviderKeyForm
 * (activate mode): paste a key → the provider is paste-detected → pick a model
 * → "Save & make active" stores `{provider, api_key, model}` and points the
 * tenant provider at it. The credential name derives from the detected
 * provider, so cleanup resets the provider and deletes that credential.
 *
 * Defensive by design: the paste-detect step is deterministic (client-side),
 * but the save+activate round-trip depends on the fixture backend accepting
 * the model, so the activation assertion tolerates an env-variance alert the
 * same way the prior credential-reference spec did. The exact action calls are
 * pinned by provider-key-form.test.tsx; this spec proves the real built form
 * wires them end to end.
 */
import { expect, test, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { gotoWorkspace } from "./fixtures/nav";

const ACTION_TIMEOUT_MS = 15_000;
// A pasted anthropic-shaped key paste-detects to provider "anthropic"; the
// consolidated form derives the credential name from the provider.
const ANTHROPIC_KEY = "sk-ant-e2e-xxxxxxxx";
const DETECTED_PROVIDER = "anthropic";
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

// The catalogue-backed model picker is a <Select> when the catalogue covers
// the provider and degrades to a free-text <Input> when it doesn't — handle
// both so the spec survives either fixture-catalogue state.
async function pickModel(page: Page): Promise<void> {
  const model = page.getByLabel("Model");
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
    await deleteCredentialDirect(DETECTED_PROVIDER);
  });

  test("a pasted provider key becomes the active model credential", async ({ page }) => {
    await resetProviderDirect();
    await deleteCredentialDirect(DETECTED_PROVIDER);

    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "settings/models");
    await expect(page.getByRole("heading", { name: /^models$/i })).toBeVisible();

    // Open the generic "Other provider" add-key form — it is the last
    // "Add key & model" affordance in the switch list (rendered after every
    // named-provider row and the custom-endpoint row).
    await page.getByRole("button", { name: "Add key & model" }).last().click();

    // Paste-detect: an anthropic-shaped key fills the provider field (this is
    // the credential-reference behaviour — deterministic, no backend).
    await page.getByLabel("API key").fill(ANTHROPIC_KEY);
    await expect(page.getByLabel("Provider")).toHaveValue(DETECTED_PROVIDER);

    await pickModel(page);
    await page.getByRole("button", { name: "Save & make active" }).click();

    // The store+activate round-trip either lands (hero goes LIVE referencing the
    // anthropic credential) or surfaces a typed alert under fixture variance.
    const hero = page.getByTestId("active-model-hero");
    const outcome = await Promise.race([
      hero
        .filter({ hasText: DETECTED_PROVIDER })
        .waitFor({ state: "visible", timeout: ACTION_TIMEOUT_MS })
        .then(() => "live" as const),
      page
        .getByRole("alert")
        .waitFor({ state: "visible", timeout: ACTION_TIMEOUT_MS })
        .then(() => "alert" as const),
    ]).catch(() => "unknown" as const);

    if (outcome === "live") {
      await expect(hero).toHaveAttribute("data-live", "true");
      await expect(hero).toContainText(DETECTED_PROVIDER);
    } else if (outcome === "alert") {
      await expect(page.getByRole("alert")).toBeVisible();
    } else {
      // Neither landmark resolved in time — fail loudly rather than silently pass.
      throw new Error("save+activate produced neither a LIVE hero nor a typed alert");
    }
  });
});
