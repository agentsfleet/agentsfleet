/**
 * logs-detail.spec.ts — fleet detail page renders SSR-authenticated content.
 *
 * Spec called for "event-row click → <Dialog> opens with payload preview".
 * The current EventsList renders event cards with an inline truncated
 * preview (`<p title="...">` on the response body) — it does not yet open
 * a modal payload viewer. That click-to-dialog feature is on the deferred
 * dashboard polish queue, not on M64_006.
 *
 * Pragmatic asserts (what WS-A actually unblocked):
 *   - SSR loads the detail page for an active fleet.
 *   - The status indicator next to the title pulses (data-state="active",
 *     WakePulse with data-live attribute) — proves the design system primitive
 *     is wired and the SSR rendered the active branch.
 *   - The Recent Activity section renders (either EmptyState or the events
 *     list scaffolding).
 *
 * The "click-row → Dialog" assertion will land in the same PR that ships
 * the EventDetail dialog (deferred — see Discovery in this milestone's
 * spec).
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedFleet } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const RENDER_TIMEOUT_MS = 15_000;

test.describe("fleet detail logs", () => {
  test("active fleet detail page renders pulsing status + recent activity", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const seeded = await seedFleet(FIXTURE_KEY.regular, ws, { name: `logs-${tag}` });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(ws, `fleets/${seeded.id}`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${seeded.id}`));

    // Header status indicator — `data-state="active"` carries the WakePulse
    // child with `data-live` set when the fleet is live.
    const statusIndicator = page.locator('[data-state="active"]').first();
    await expect(statusIndicator).toBeVisible({ timeout: RENDER_TIMEOUT_MS });
    await expect(statusIndicator.locator("[data-live]")).toBeVisible();

    // Recent Activity section is always present on the detail page; assert
    // its label so a section-rename surfaces here.
    const recentActivity = page.getByRole("region", { name: "Recent Activity" });
    await expect(recentActivity).toBeVisible();
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws);
  });
});
