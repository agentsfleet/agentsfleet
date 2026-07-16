/**
 * integrations-nav.spec.ts — the sidebar exposes the two settings surfaces the
 * M102 credential/integration work split apart, and each renders.
 *
 * After the Models consolidation the nav lists "Models"
 * (/w/<id>/settings/models) and "Integrations" (/w/<id>/integrations) as
 * sibling items. This spec drives the nav itself (not a deep-link goto) so a
 * broken nav label or href is caught, then asserts each destination renders its
 * stable landmark — the Models table and the Integrations connectors region.
 * Read-only: no tenant or integration state is mutated.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("settings navigation — Models + Integrations", () => {
  test("the nav routes to Models and to Integrations, each rendering", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    // `/` redirects to the first owned workspace (`/w/<id>`), mounting the nav.
    await page.goto("/");

    // Integrations — nav click lands on /w/<id>/integrations and renders connectors.
    await page.getByRole("link", { name: "Integrations" }).first().click();
    await expect(page).toHaveURL(workspaceUrlPattern("integrations"));
    await expect(page.getByRole("heading", { name: /^integrations$/i })).toBeVisible();
    await expect(page.getByTestId("integrations-page")).toBeVisible();

    // Models — sibling nav item lands on /w/<id>/settings/models and renders the registry.
    await page.getByRole("link", { name: "Models" }).first().click();
    await expect(page).toHaveURL(workspaceUrlPattern("settings/models"));
    await expect(page.getByRole("heading", { name: /^models$/i })).toBeVisible();
    await expect(page.getByRole("table", { name: "Models" })).toBeVisible();
  });
});
