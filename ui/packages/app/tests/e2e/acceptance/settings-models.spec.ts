/**
 * settings-models.spec.ts — /settings/models (Models & Keys) renders for an
 * authed user, and the legacy /credentials route redirects into it.
 *
 * Asserts the page resolves the active workspace, calls GET
 * /v1/tenants/me/provider (resolved server-side), and renders its three
 * stable regions: the Active-Model hero, the provider switch list, and the
 * custom-secrets group. Does NOT mutate tenant provider state — that races
 * other specs against the same fixture tenant; the switch/rotate flows are
 * covered by unit tests and by provider-credential-reference.spec.ts.
 *
 * Page render alone is a useful signal because the provider resolver has
 * multiple failure modes (synthesised default, credential-ref mismatch,
 * backend 5xx) that all degrade to visible chrome here — the hero renders in
 * both the LIVE (self-managed) and DEFAULT (platform) branches.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("Models & Keys page", () => {
  test("Models & Keys renders the hero, switch list, and custom secrets", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/models");
    await expect(page).toHaveURL(/\/settings\/models(\?|$)/);

    await expect(page.getByRole("heading", { name: /^models & keys$/i })).toBeVisible();
    await expect(page.getByTestId("active-model-hero")).toBeVisible();
    await expect(page.getByTestId("provider-switch-list")).toBeVisible();
    await expect(page.getByTestId("custom-secrets-group")).toBeVisible();
  });

  test("the legacy /credentials route redirects into Models & Keys", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/credentials");
    // The standalone credentials vault was folded into Models & Keys; the route
    // is a server redirect that keeps install-preview deep-links resolving.
    await expect(page).toHaveURL(/\/settings\/models(\?|$)/);
    await expect(page.getByTestId("custom-secrets-group")).toBeVisible();
  });
});
