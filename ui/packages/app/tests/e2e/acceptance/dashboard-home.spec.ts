/**
 * dashboard-home.spec.ts — `/` renders for the fixture user.
 *
 * Two render modes worth covering, both via the same authed fixture:
 *   - Empty: no zombies → FirstInstallCard renders ("First wake" eyebrow).
 *   - Populated: ≥1 zombie → StatusCard tiles render (Live / Paused /
 *     Stopped / Balance).
 *
 * The persistent fixture's state is not deterministic between specs, so
 * this test asserts the *union*: PageHeader is visible, and either the
 * StatusCard group or the FirstInstall section renders. Either render
 * path failing without the other landing is a regression in the
 * Suspense boundary or the `StatusTiles` data fetch.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("dashboard home", () => {
  test("`/` renders header + status tiles or first-install card", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/");
    await expect(page).toHaveURL(/\/$|\/(\?|$)/);

    await expect(page.getByRole("heading", { name: /^dashboard$/i })).toBeVisible();

    // Either render path is correct. data-testid="status-card" comes from
    // the StatusCard primitive; role=region narrows the FirstInstallCard
    // section and avoids matching its command block label.
    const tiles = page.getByTestId("status-card");
    const firstInstall = page.getByRole("region", { name: "Install your first zombie" });
    await expect(tiles.first().or(firstInstall)).toBeVisible({ timeout: 15_000 });
  });
});
