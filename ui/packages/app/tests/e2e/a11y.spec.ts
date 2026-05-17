import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

/*
 * App accessibility audit on unauthenticated routes. Authenticated
 * dashboard routes are gated by Clerk; once @clerk/testing helpers are
 * wired into the e2e harness, extend ROUTES with `/zombies`,
 * `/zombies/new`, `/zombies/[id]`, `/settings`.
 *
 * The structural rules in DISABLED_RULES_PENDING_CLEANUP are known
 * violations on the Clerk-hosted sign-in/sign-up shells. The shells
 * are rendered by Clerk's SDK without our usual landmark/heading
 * wrapping; the cleanup is to wrap them in a `<main>` with an `<h1>`
 * (sr-only is fine) inside the route layout. Each rule should be
 * removed from this list as the wrapper lands.
 */

const DISABLED_RULES_PENDING_CLEANUP = [
  "landmark-one-main", // /sign-in, /sign-up: Clerk shell has no <main>
  "page-has-heading-one", // /sign-in, /sign-up: Clerk shell has no <h1>
  "region", // /sign-in, /sign-up: content sits outside landmarks
];

const ROUTES: Array<{ path: string; label: string }> = [
  { path: "/sign-in", label: "Sign-in" },
  { path: "/sign-up", label: "Sign-up" },
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
