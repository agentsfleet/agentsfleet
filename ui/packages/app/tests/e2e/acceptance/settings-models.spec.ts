/**
 * settings-models.spec.ts — /settings/models (Models) renders for an
 * authed user.
 *
 * Asserts the page resolves the active workspace, calls GET
 * /v1/tenants/me/provider (resolved server-side), and renders its two
 * stable model-registry table. The
 * custom-secrets group moved out to its own /secrets page (see
 * secrets-lifecycle.spec.ts), so this page renders no secrets content at all.
 * Does NOT mutate tenant provider state — that races other specs against the
 * same fixture tenant; the switch/rotate flows are covered by unit tests and
 * by provider-credential-reference.spec.ts.
 *
 * Page render alone is a useful signal because the provider resolver has
 * multiple failure modes (synthesised default, secret-ref mismatch,
 * backend 5xx) that all degrade to visible chrome here.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { gotoWorkspace, workspaceUrlPattern } from "./fixtures/nav";

test.describe("Models page", () => {
  test("Models renders the registry, with no secrets content", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "settings/models");
    await expect(page).toHaveURL(workspaceUrlPattern("settings/models"));

    await expect(page.getByRole("heading", { name: /^models$/i })).toBeVisible();
    await expect(page.getByRole("table", { name: "Models" })).toBeVisible();
    await expect(page.getByTestId("custom-secrets-group")).toHaveCount(0);
  });
});
