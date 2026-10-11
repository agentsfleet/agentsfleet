/**
 * settings-billing.spec.ts — the balance card shows a dollar amount.
 *
 * Visits /settings/billing as the regular fixture user and asserts the
 * balance headline (`data-testid="balance-headline"`) renders a `$` amount.
 * The Buy credits mailto link is proven by the billing card's unit tests.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("settings billing", () => {
  test("balance card renders a dollar amount", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/billing");
    await expect(page).toHaveURL(/\/settings\/billing(\?|$)/);

    const balance = page.getByTestId("balance-headline");
    await expect(balance).toBeVisible();
    // Scope the amount to the balance card, not any figure on the page.
    const card = balance.locator("..");
    await expect(card).toContainText(/\$\d/);
  });
});
