import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

/*
 * App accessibility audit on unauthenticated routes. Authenticated
 * dashboard routes are gated by Clerk; once @clerk/testing helpers are
 * wired into the e2e harness, extend ROUTES with `/zombies`,
 * `/zombies/new`, `/zombies/[id]`, `/settings`.
 */

const ROUTES: Array<{ path: string; label: string }> = [
  { path: "/sign-in", label: "Sign-in" },
  { path: "/sign-up", label: "Sign-up" },
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
