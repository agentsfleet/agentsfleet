import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const ROUTES: Array<{ path: string; label: string }> = [
  { path: "/", label: "Home" },
  { path: "/agents", label: "Fleets" },
  { path: "/privacy", label: "Privacy" },
  { path: "/terms", label: "Terms" },
  { path: "/_design-system", label: "Design gallery" },
  { path: "/unavailable-page", label: "Page not found" },
];
const VIEWPORTS = [1440, 390, 320];

for (const { path, label } of ROUTES) {
  for (const theme of ["dark", "light"]) {
  for (const width of VIEWPORTS) {
  test(`${label} (${path}) has zero axe violations in ${theme} at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(path);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.evaluate((value) => document.documentElement.setAttribute("data-theme", value), theme);
    await page.evaluate(async () => {
      const transitions = document.getAnimations().filter((animation) =>
        animation.effect?.getTiming().iterations !== Infinity);
      await Promise.all(transitions.map((animation) => animation.finished));
    });
    const results = await new AxeBuilder({ page })
      .analyze();
    expect(results.violations).toEqual([]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
  }
  }
}
