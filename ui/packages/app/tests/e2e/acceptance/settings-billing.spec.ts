/**
 * settings-billing.spec.ts — settled account spending is inspectable.
 *
 * Visits the real Billing page and checks full-period reporting, filtering,
 * the shared views and current period/fleet controls.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { expectAccessible, expectNoPageOverflow } from "./fixtures/accessibility";

const PERIOD_CHARGE = "Settled charges";
const BILLING_PATH = "/settings/billing";
const FILTER_LABEL = { period: "Period", fleet: "Fleet" } as const;
const ROLE = { combobox: "combobox", button: "button", heading: "heading" } as const;
const NAVIGATION = { expanded: "expanded", collapsed: "collapsed" } as const;
const DESKTOP_HEIGHT = 1000;
const VIEWPORT = {
  desktop: { width: 1440, height: DESKTOP_HEIGHT }, intermediate: { width: 900, height: DESKTOP_HEIGHT },
  mobile: { width: 390, height: 844 }, narrow: { width: 320, height: 844 },
} as const;
const LAYOUTS = [
  { name: "desktop-open", viewport: VIEWPORT.desktop, navigation: NAVIGATION.expanded },
  { name: "desktop-closed", viewport: VIEWPORT.desktop, navigation: NAVIGATION.collapsed },
  { name: "intermediate-open", viewport: VIEWPORT.intermediate, navigation: NAVIGATION.expanded },
  { name: "intermediate-closed", viewport: VIEWPORT.intermediate, navigation: NAVIGATION.collapsed },
  { name: "mobile", viewport: VIEWPORT.mobile, navigation: NAVIGATION.collapsed },
  { name: "narrow", viewport: VIEWPORT.narrow, navigation: NAVIGATION.collapsed },
] as const;

test.use({ reducedMotion: "reduce" });

test.describe("settings billing", () => {
  test("settled report renders an amount and accessible review controls", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(BILLING_PATH);
    await expect(page).toHaveURL(/\/settings\/billing(\?|$)/);

    await expect(page.getByRole(ROLE.heading, { name: "Usage", exact: true })).toBeVisible();
    await expect(page.getByText(PERIOD_CHARGE, { exact: true })).toBeVisible();
    const summary = page.locator('[aria-label="Period totals"]');
    await expect(summary).toContainText(/\$\d/);
    await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.period, exact: true })).toBeVisible();
    await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.fleet, exact: true })).toBeVisible();
    await expect(page.getByRole(ROLE.button, {name: "More filters", exact: true})).toHaveCount(0);
    await expect(page.getByRole("link", { name: "Add credits" })).toHaveCount(0);
    await expect(page.getByRole(ROLE.button, {name: "Charge details", exact: true})).toHaveCount(0);
    await expect(page.getByRole(ROLE.combobox, {name: FILTER_LABEL.fleet})).toContainText("All Fleets");
    await expect(page.getByText(/Totals exclude pending or unavailable provider usage/)).toBeVisible();
    await expect(page.getByRole("table", {name: "Spending by fleet", exact: true})
      .getByRole("columnheader")).toHaveText(["Fleet", "Model", "Activities", "Charged"]);
    await expect(page.getByRole("figure", {name: "Daily spend"})).toBeVisible();
    await page.getByRole(ROLE.combobox, { name: FILTER_LABEL.period, exact: true }).click();
    await page.getByRole("option", { name: "Last 7 days", exact: true }).click();
    await expect(page).toHaveURL(/period=week/);
    await expect(page.getByText(PERIOD_CHARGE, { exact: true })).toBeVisible();
  });

  for (const theme of ["dark", "light"] as const) {
    test(`shared Billing components remain accessible in ${theme} with expanded, collapsed and mobile navigation`, async ({ page }, testInfo) => {
      await page.setViewportSize(VIEWPORT.desktop);
      await signInAs(page, FIXTURE_KEY.regular);
      await page.goto(BILLING_PATH);
      await expect(page.getByText(PERIOD_CHARGE, { exact: true })).toBeVisible();
      await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme);
      for (const layout of LAYOUTS) {
        await page.setViewportSize(layout.viewport);
        if (layout.viewport.width >= VIEWPORT.intermediate.width) {
          const toggle = page.getByRole(ROLE.button, {
            name: layout.navigation === NAVIGATION.expanded ? "Expand navigation" : "Collapse navigation", exact: true,
          });
          if (await toggle.isVisible()) await toggle.click();
          const expanded = layout.navigation === NAVIGATION.expanded;
          await expect(page.getByRole(ROLE.button, {name: expanded ? "Collapse navigation" : "Expand navigation", exact: true}))
            .toHaveAttribute("aria-expanded", String(expanded));
        }
        await page.getByRole(ROLE.combobox, { name: FILTER_LABEL.fleet, exact: true }).scrollIntoViewIfNeeded();
        await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.fleet, exact: true })).toBeInViewport();
        const totals = await page.locator('[aria-label="Period totals"] > div').evaluateAll(nodes => nodes.map(node => {
          const box = node.getBoundingClientRect();
          return {top: box.top, left: box.left, right: box.right};
        }));
        expect(totals).toHaveLength(3);
        const [charge, activities, tokens] = totals;
        if (charge === undefined || activities === undefined || tokens === undefined) throw new Error("Period totals lost a metric");
        if (layout.viewport.width >= VIEWPORT.intermediate.width) {
          expect(Math.max(...totals.map(metric => metric.top)) - Math.min(...totals.map(metric => metric.top))).toBeLessThan(1);
        } else {
          // Below the summary breakpoint the charge spans the first row and
          // Activities and Tokens share the second, one column each.
          expect(activities.top).toBeGreaterThan(charge.top);
          expect(Math.abs(tokens.top - activities.top)).toBeLessThan(1);
          expect(Math.abs(activities.left - charge.left)).toBeLessThan(1);
          expect(tokens.left).toBeGreaterThan(activities.right - 1);
          expect(Math.abs(tokens.right - charge.right)).toBeLessThan(1);
        }
        const activity = page.getByRole("table", { name: "Spending by fleet", exact: true });
        await expect(activity.getByRole("columnheader")).toHaveCount(4);
        const first = activity.locator("tbody > tr").first().getByRole("cell");
        if (await first.count() === 4) {
          await expect(first.nth(2)).toBeVisible();
          await expect(first.nth(3)).toBeVisible();
        }
        await expectAccessible(page, `Billing ${theme} ${layout.name}`);
        await expectNoPageOverflow(page);
        await page.screenshot({ path: testInfo.outputPath(`billing-${theme}-${layout.name}.png`), fullPage: true });
      }
    });
  }
});
