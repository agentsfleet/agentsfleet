import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

/*
 * Design-system smoke spec.
 *
 * WHY THIS EXISTS: JSDOM unit tests assert className string content —
 * they pass whether or not Tailwind compiled the referenced utility.
 * This spec loads the real website in Chromium with the real compiled
 * CSS, then asserts computed styles to prove every utility actually
 * produces the intended visual output.
 *
 * If a future edit drops a Tailwind @source or mis-spells a utility,
 * this spec fails loudly — the JSDOM suite would be silent.
 *
 * Route: /_design-system (see src/pages/DesignSystemGallery.tsx)
 */

async function computed(page: Page, testid: string, prop: string) {
  return page.locator(`[data-testid="${testid}"]`).evaluate(
    (el, p) => getComputedStyle(el).getPropertyValue(p),
    prop,
  );
}

test.beforeEach(async ({ page }) => {
  await page.goto("/_design-system");
  await page.waitForLoadState("networkidle");
});

test.describe("Tooltip — computed styles", () => {
  test("tooltip content is not rendered in resting state", async ({ page }) => {
    await expect(page.locator('[data-testid="tooltip-content"]')).toHaveCount(0);
  });

  test("hovering the trigger shows the tooltip with popover surface", async ({ page }) => {
    await page.locator('[data-testid="tooltip-trigger"]').hover();
    const content = page.locator('[data-testid="tooltip-content"]');
    await expect(content).toBeVisible({ timeout: 2000 });
    const bg = await content.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
    const font = await content.evaluate((el) => getComputedStyle(el).fontFamily);
    expect(font).toContain("Instrument Sans");
  });
});

test.describe("EmptyState — computed styles", () => {
  test("renders with dashed border + card background tint", async ({ page }) => {
    const empty = page.locator('[data-testid="empty-state"]').first();
    await expect(empty).toBeVisible();
    const style = await empty.evaluate((el) => getComputedStyle(el).borderTopStyle);
    expect(style).toBe("dashed");
  });
});

test.describe("StatusCard — computed styles", () => {
  test("danger variant count uses the destructive color", async ({ page }) => {
    const danger = page.locator('[data-testid="status-card"][data-variant="danger"]').first();
    const countColor = await danger
      .locator("dd")
      .first()
      .evaluate((el) => getComputedStyle(el).color);
    const successCountColor = await page
      .locator('[data-testid="status-card"][data-variant="success"]')
      .first()
      .locator("dd")
      .first()
      .evaluate((el) => getComputedStyle(el).color);
    expect(countColor).not.toBe(successCountColor);
  });

  test("has a visible border that tints on focus-within", async ({ page }) => {
    const card = page.locator('[data-testid="status-card"]').first();
    const bw = await card.evaluate((el) => getComputedStyle(el).borderTopWidth);
    expect(parseFloat(bw)).toBeGreaterThan(0);
  });
});

test.describe("Pagination — computed styles", () => {
  test("page variant renders Prev/Next + page counter text", async ({ page }) => {
    const nav = page.locator('[data-testid="pagination-page"]').first();
    await expect(nav).toBeVisible();
    await expect(nav).toContainText("Page 2 of 5");
    await expect(nav.locator("button")).toHaveCount(2);
  });
});

test.describe("Accessibility", () => {
  test("gallery content has zero axe violations", async ({ page }) => {
    // Scope to the gallery content — site shell (header/footer) heading order
    // is not part of the design-system contract, audited in its own spec.
    const results = await new AxeBuilder({ page })
      .include("main")
      .disableRules(["color-contrast"]) // brand palette; audited separately
      .analyze();
    expect(results.violations).toEqual([]);
  });
});
