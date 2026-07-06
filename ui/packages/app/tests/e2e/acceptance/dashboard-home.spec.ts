/**
 * dashboard-home.spec.ts — `/` redirects to the workspace home (`/w/<id>`),
 * which renders for the fixture user.
 *
 * Two render modes worth covering, both via the same authed fixture:
 *   - Empty: no Fleets → the FirstInstall gallery renders ("Start your fleet").
 *   - Populated: ≥1 Fleet → StatusCard tiles render (Live / Paused /
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
import { workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("dashboard home", () => {
  test("`/` renders header + status tiles or first-install card", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    // `/` redirects to the first owned workspace home (`/w/<id>`).
    await page.goto("/");
    await expect(page).toHaveURL(workspaceUrlPattern());

    await expect(page.getByRole("heading", { name: /^dashboard$/i })).toBeVisible();

    // Either render path is correct. data-testid="status-card" comes from
    // the StatusCard primitive; role=region narrows the FirstInstall
    // section and avoids matching its command block label.
    const tiles = page.getByTestId("status-card");
    const firstInstall = page.getByLabel("Start your fleet");
    await expect(tiles.first().or(firstInstall)).toBeVisible({ timeout: 15_000 });
  });
});
