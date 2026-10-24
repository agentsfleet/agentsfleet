/**
 * settings-billing.spec.ts — settled account spending is inspectable.
 *
 * Visits the real Billing page and checks full-period reporting, filtering,
 * the shared views and the existing support action.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

const SETTLED_SPEND = "Settled spend";
const BILLING_PATH = "/settings/billing";
const FILTER_LABEL = { period: "Period", fleet: "Fleet", model: "Model" } as const;
const ROLE = { combobox: "combobox", button: "button", heading: "heading" } as const;
const NAVIGATION = { expanded: "expanded", collapsed: "collapsed", mobile: "mobile" } as const;
const VIEWPORT = { desktop: { width: 1440, height: 1000 }, mobile: { width: 390, height: 844 } } as const;

test.use({ reducedMotion: "reduce" });

test.describe("settings billing", () => {
  test("settled report renders an amount and accessible review controls", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(BILLING_PATH);
    await expect(page).toHaveURL(/\/settings\/billing(\?|$)/);

    await expect(page.getByText(SETTLED_SPEND, { exact: true })).toBeVisible();
    const summary = page.locator("[data-dashboard-panel]").filter({ hasText: "Billed activities" });
    await expect(summary).toContainText(/\$\d/);
    await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.period, exact: true })).toBeVisible();
    await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.fleet, exact: true })).toBeVisible();
    await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.model, exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "Add credits" })).toHaveAttribute("href", /^mailto:/);
    await expect(page.getByText(/Pending or unavailable provider usage/)).toBeVisible();
    await page.getByRole("tab", { name: "Spending", exact: true }).click();
    await expect(page.getByRole(ROLE.heading, { name: "By fleet", exact: true })).toBeVisible();
    await expect(page.getByRole(ROLE.heading, { name: "By model", exact: true })).toBeVisible();
    await page.getByRole(ROLE.combobox, { name: FILTER_LABEL.period, exact: true }).click();
    await page.getByRole("option", { name: "Today", exact: true }).click();
    await expect(page).toHaveURL(/period=today/);
    await expect(page.getByText(SETTLED_SPEND, { exact: true })).toBeVisible();
  });

  for (const theme of ["dark", "light"] as const) {
    test(`shared Billing components remain accessible in ${theme} with expanded, collapsed and mobile navigation`, async ({ page }, testInfo) => {
      await page.setViewportSize(VIEWPORT.desktop);
      await signInAs(page, FIXTURE_KEY.regular);
      await page.goto(BILLING_PATH);
      await expect(page.getByText(SETTLED_SPEND, { exact: true })).toBeVisible();
      await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme);
      for (const state of Object.values(NAVIGATION)) {
        if (state === NAVIGATION.collapsed) {
          await page.getByRole(ROLE.button, { name: "Collapse sidebar", exact: true }).click();
          await expect(page.getByRole(ROLE.button, { name: "Expand sidebar", exact: true })).toBeVisible();
        }
        if (state === NAVIGATION.mobile) await page.setViewportSize(VIEWPORT.mobile);
        await page.getByRole(ROLE.combobox, { name: FILTER_LABEL.fleet, exact: true }).scrollIntoViewIfNeeded();
        await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.fleet, exact: true })).toBeInViewport();
        await expectAccessible(page, `Billing ${theme} ${state}`);
        await expectNoPageOverflow(page);
        await page.screenshot({ path: testInfo.outputPath(`billing-${theme}-${state}.png`), fullPage: true });
      }
    });
  }
});
