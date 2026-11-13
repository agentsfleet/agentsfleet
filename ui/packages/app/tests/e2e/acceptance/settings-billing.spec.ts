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

const PERIOD_CHARGE = "Settled charges";
const BILLING_PATH = "/settings/billing";
const FILTER_LABEL = { period: "Period", fleet: "Fleet", model: "Model" } as const;
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
    await page.getByRole(ROLE.button, {name: "More filters", exact: true}).click();
    await expect(page.getByRole(ROLE.combobox, { name: FILTER_LABEL.model, exact: true })).toBeVisible();
    await expectAccessible(page, "Usage additional filters");
    await page.keyboard.press("Escape");
    await expect(page.getByRole(ROLE.button, {name: "More filters", exact: true})).toBeFocused();
    await expect(page.getByRole("link", { name: "Add credits" })).toHaveAttribute("href", /^mailto:/);
    await expect(page.getByText(/Totals exclude pending or unavailable provider usage/)).toBeVisible();
    await expect(page.getByRole("table", {name: "Spending by fleet", exact: true})
      .getByRole("columnheader")).toHaveText(["Fleet", "Activities", "Charged"]);
    await expect(page.getByRole("figure", {name: "Daily spend"})).toContainText("Daily spending is not available yet.");
    await page.getByRole(ROLE.button, {name: "Charge details", exact: true}).click();
    await expect(page.getByRole("table", { name: "Billing activity", exact: true })).toBeVisible();
    await expect(page.getByText(/Charged by fleet or setup chat|No settled spending/)).toBeVisible();
    await expect(page.getByText(/Charged by model|No settled spending/)).toBeVisible();
    await page.getByRole(ROLE.combobox, { name: FILTER_LABEL.period, exact: true }).click();
    await page.getByRole("option", { name: "Today", exact: true }).click();
    await expect(page).toHaveURL(/period=today/);
    await expect(page.getByText(PERIOD_CHARGE, { exact: true })).toBeVisible();
  });

  for (const theme of ["dark", "light"] as const) {
    test(`shared Billing components remain accessible in ${theme} with expanded, collapsed and mobile navigation`, async ({ page }, testInfo) => {
      await page.setViewportSize(VIEWPORT.desktop);
      await signInAs(page, FIXTURE_KEY.regular);
      await page.goto(BILLING_PATH);
      await expect(page.getByText(PERIOD_CHARGE, { exact: true })).toBeVisible();
      await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme);
      await page.getByRole(ROLE.button, {name: "Charge details", exact: true}).click();
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
        const totals = await page.locator('[aria-label="Period totals"] > div').evaluateAll(nodes =>
          nodes.map(node => ({top: node.getBoundingClientRect().top, left: node.getBoundingClientRect().left})));
        expect(totals).toHaveLength(3);
        if (layout.viewport.width >= VIEWPORT.intermediate.width) {
          expect(Math.max(...totals.map(metric => metric.top)) - Math.min(...totals.map(metric => metric.top))).toBeLessThan(1);
        } else {
          expect(totals[1]!.top).toBeGreaterThan(totals[0]!.top);
          expect(totals[2]!.top).toBeGreaterThan(totals[1]!.top);
          expect(Math.max(...totals.map(metric => metric.left)) - Math.min(...totals.map(metric => metric.left))).toBeLessThan(1);
        }
        const activity = page.getByRole("table", { name: "Billing activity", exact: true });
        await expect(activity.getByRole("columnheader")).toHaveCount(5);
        const first = activity.locator("tbody > tr").first().getByRole("cell");
        if (await first.count() === 5) {
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
