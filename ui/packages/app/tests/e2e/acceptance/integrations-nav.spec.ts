/**
 * integrations-nav.spec.ts — the sidebar exposes the two settings surfaces the
 * M102 credential/integration work split apart, and each renders.
 *
 * After the Models & Keys consolidation the nav lists "Models & Keys"
 * (/settings/models) and "Integrations" (/integrations) as sibling items.
 * This spec drives the nav itself (not a deep-link goto) so a broken nav
 * label or href is caught, then asserts each destination renders its stable
 * landmark — the Models & Keys hero and the Integrations connectors region.
 * Read-only: no tenant or integration state is mutated.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("settings navigation — Models & Keys + Integrations", () => {
  test("the nav routes to Models & Keys and to Integrations, each rendering", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/");

    // Integrations — nav click lands on /integrations and renders connectors.
    await page.getByRole("link", { name: "Integrations" }).first().click();
    await expect(page).toHaveURL(/\/integrations(\?|$)/);
    await expect(page.getByRole("heading", { name: /^integrations$/i })).toBeVisible();
    await expect(page.getByTestId("integrations-page")).toBeVisible();

    // Models & Keys — sibling nav item lands on /settings/models and renders the hero.
    await page.getByRole("link", { name: "Models & Keys" }).first().click();
    await expect(page).toHaveURL(/\/settings\/models(\?|$)/);
    await expect(page.getByRole("heading", { name: /^models & keys$/i })).toBeVisible();
    await expect(page.getByTestId("active-model-hero")).toBeVisible();
  });
});
