import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const ROUTES: Array<{ path: string; label: string }> = [
  { path: "/", label: "Home" },
  { path: "/fleets", label: "Fleets" },
  { path: "/privacy", label: "Privacy" },
  { path: "/terms", label: "Terms" },
  { path: "/about", label: "About" },
];

for (const { path, label } of ROUTES) {
  for (const theme of ["dark", "light"]) {
  test(`${label} (${path}) has zero axe violations in ${theme}`, async ({ page }) => {
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
  });
  }
}
