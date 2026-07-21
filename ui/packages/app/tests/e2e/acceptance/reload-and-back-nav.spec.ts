/**
 * reload-and-back-nav.spec.ts — session survives hard reload + soft route nav.
 *
 * Two failure modes this catches:
 *   1. Hard reload (`page.reload()`) — clerkMiddleware must re-resolve the
 *      session cookie SSR. A regression that drops the `azp` claim, races
 *      cookie write, or fails to rehydrate the detail-page React Server
 *      Component tree lands as a /sign-in redirect or empty render.
 *   2. Soft App-Router nav away and back — Next caches RSC payloads
 *      per-segment. A regression in revalidatePath or the workspace
 *      resolver shows up as stale data after returning to /w/<id>/fleets/<id>.
 *
 * Seeds a fleet via API for speed (install-form coverage already lives
 * in login-install-lifecycle.spec.ts); the signal here is the navigation
 * mechanics, not the install action.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedFleet } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const NAV_TIMEOUT_MS = 15_000;

test.describe("reload + back-nav mid-session", () => {
  test("fleet detail survives hard reload and a soft round-trip", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const name = `nav-${Math.random().toString(36).slice(2, 8)}`;
    const seeded = await seedFleet(FIXTURE_KEY.regular, ws, { name });

    await signInAs(page, FIXTURE_KEY.regular);
    // Trigger is a rail destination now; the query names the view so both the
    // hard reload and the back-nav below re-resolve the same surface.
    await page.goto(workspaceHref(ws, `fleets/${seeded.id}?view=trigger`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${seeded.id}`));
    // `.first()`: during reload hydration the framework can briefly hold a
    // second detached copy of the card, and strict mode trips on the pair
    // before visibility resolves.
    const triggerSection = page.getByLabel("Configured triggers").first();
    await expect(triggerSection).toBeVisible({ timeout: 15_000 });

    // 1. Hard reload — server re-resolves cookie + RSC tree.
    await page.reload();
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${seeded.id}`));
    await expect(triggerSection).toBeVisible({ timeout: NAV_TIMEOUT_MS });

    // 2. Soft nav away (/events) and back. Uses page.goto rather than a
    // sidebar click — the navigation mechanics are the regression signal;
    // selector flakiness on the Shell nav is a different surface.
    await page.goto(workspaceHref(ws, "events"));
    await expect(page).toHaveURL(workspaceUrlPattern("events"));
    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();

    await page.goto(workspaceHref(ws, `fleets/${seeded.id}?view=trigger`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${seeded.id}`));
    await expect(triggerSection).toBeVisible({ timeout: NAV_TIMEOUT_MS });
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, "nav-");
  });
});
