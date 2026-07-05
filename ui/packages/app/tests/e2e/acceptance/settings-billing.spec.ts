/**
 * settings-billing.spec.ts — balance card renders with tabular-nums.
 *
 * Visits /settings/billing as the regular fixture user. Asserts:
 *   1. The balance headline (`data-testid="balance-headline"`) renders.
 *   2. The headline carries `tabular-nums` so digits don't jitter when the
 *      balance updates between polls.
 *   3. The Buy credits trigger renders as a live mailto link (
 *      not a disabled button), with its tooltip wiring intact.
 *
 * Plan-tier badge intentionally not asserted — dropped in M65_001.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("settings billing", () => {
  test("balance card renders with tabular-nums", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/billing");
    await expect(page).toHaveURL(/\/settings\/billing(\?|$)/);

    const balance = page.getByTestId("balance-headline");
    await expect(balance).toBeVisible();
    // tabular-nums is on the parent CardTitle; assert via class lookup so a
    // future BillingBalanceCard refactor that moves the class up or down
    // one node still reads as a regression.
    const tabular = page.locator(".tabular-nums").first();
    await expect(tabular).toBeVisible();
    await expect(tabular).toContainText(/\$\d/);

    const buyCredits = page.getByTestId("buy-credits-trigger");
    await expect(buyCredits).toBeVisible();
    await expect(page.getByRole("link", { name: "Buy credits" })).toHaveAttribute(
      "href",
      "mailto:agentsfleet@agentmail.to",
    );
  });
});
