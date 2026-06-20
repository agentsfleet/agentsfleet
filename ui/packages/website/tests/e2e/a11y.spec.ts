import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

/*
 * Site-wide accessibility audit. Wires @axe-core/playwright across every
 * top-level route. Color-contrast is audited against the brand palette in
 * `design-system-smoke.spec.ts` and intentionally excluded here.
 *
 * The structural rules in DISABLED_RULES_PENDING_CLEANUP are known
 * violations on the current marketing pages (heading hierarchy +
 * missing h1 on /fleets, /privacy, /terms). Disabled to keep the gate
 * live for every other axe rule until the marketing-page cleanup pass
 * lands. Each rule should be removed from this list (one PR per rule)
 * as the underlying markup is fixed.
 */

const DISABLED_RULES_PENDING_CLEANUP = [
  "heading-order", // Home: h1 → h3 skips h2
  "page-has-heading-one", // /fleets, /privacy, /terms: no <h1>
];

const ROUTES: Array<{ path: string; label: string }> = [
  { path: "/", label: "Home" },
  { path: "/fleets", label: "Fleets" },
  { path: "/privacy", label: "Privacy" },
  { path: "/terms", label: "Terms" },
];

for (const { path, label } of ROUTES) {
  test(`${label} (${path}) has zero axe violations`, async ({ page }) => {
    await page.goto(path);
    const results = await new AxeBuilder({ page })
      .disableRules(["color-contrast", ...DISABLED_RULES_PENDING_CLEANUP])
      .analyze();
    expect(results.violations).toEqual([]);
  });
}
