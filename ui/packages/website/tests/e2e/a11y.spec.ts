import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

/*
 * Site-wide accessibility audit. Wires @axe-core/playwright across every
 * top-level route. Color-contrast is audited against the brand palette in
 * `design-system-smoke.spec.ts` and intentionally excluded here.
 */

const ROUTES: Array<{ path: string; label: string }> = [
  { path: "/", label: "Home" },
  { path: "/agents", label: "Agents" },
  { path: "/privacy", label: "Privacy" },
  { path: "/terms", label: "Terms" },
];

for (const { path, label } of ROUTES) {
  test(`${label} (${path}) has zero axe violations`, async ({ page }) => {
    await page.goto(path);
    const results = await new AxeBuilder({ page })
      .disableRules(["color-contrast"])
      .analyze();
    expect(results.violations).toEqual([]);
  });
}
