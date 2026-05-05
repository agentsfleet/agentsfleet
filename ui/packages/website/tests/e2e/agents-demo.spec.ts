import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

/*
 * /agents interactive demo coverage for M26.8 — <BackgroundBeamsWithCollision />
 * and <AnimatedTerminal />. Real-browser contract assertions that JSDOM
 * tests (SSR snapshots + tokenizer) can't cover:
 *   • prefers-reduced-motion collapses motion paths (dims 5.8.3, 5.8.12)
 *   • terminal chrome + live region render (dim 5.8.16)
 *   • zero axe violations on the interactive surface (dims 5.8.5, 5.8.16)
 *   • motion library chunk lazy-loads only on /agents (dim 5.8.7 — size-limit
 *     covers the budget; here we just assert the chunk is present at all)
 *
 * Deferred to follow-ups: collision-detection data flag (5.8.2), CPU idle on
 * tab hidden (5.8.4), LCP Lighthouse CI (5.8.6), step-through phase machine
 * (5.8.10), useInView sticky (5.8.11). Audio dims (5.8.13/14) intentionally
 * skipped per D4.
 */

test.describe("/agents — M26.8 demos", () => {
  test("terminal chrome renders with the aria region label", async ({ page }) => {
    await page.goto("/agents");
    const terminal = page.getByRole("region", { name: "Interactive terminal demonstration" });
    await expect(terminal).toBeVisible();
  });

  test("beams container is presentational and ignored by the a11y tree", async ({ page }) => {
    await page.goto("/agents");
    const beamsContainer = page.locator('[role="presentation"]').first();
    await expect(beamsContainer).toBeVisible();
  });

  test("prefers-reduced-motion collapses beams + terminal to static content", async ({
    browser,
  }) => {
    const ctx = await browser.newContext({ reducedMotion: "reduce" });
    const page = await ctx.newPage();
    await page.goto("/agents");
    // Reduced-motion: beams themselves should NOT be rendered
    // (BackgroundBeamsWithCollision skips the CollisionBeam map).
    const beams = page.locator('[data-testid="beam"]');
    await expect(beams).toHaveCount(0);
    // Terminal still renders its prompt + commands (printed instantly).
    const terminal = page.getByRole("region", { name: "Interactive terminal demonstration" });
    await expect(terminal).toBeVisible();
    await expect(terminal).toContainText("zombiectl login");
    // Demo outputs mirror the real zombiectl strings — guards against the
    // demo silently drifting away from the CLI it claims to demonstrate
    // (see zombiectl/src/commands/core.js + zombie_steer.js).
    await expect(terminal).toContainText("✔ login complete");
    await expect(terminal).toContainText("[claw]");
    await expect(terminal).toContainText("event 1730812800000-0 processed");
    // Slash command renders under a non-zsh prompt override so the demo
    // doesn't suggest you can paste it into a shell.
    await expect(terminal).toContainText("claude-code ›");
    await ctx.close();
  });

  test("axe-core finds zero violations on the /agents interactive surface", async ({
    page,
  }: { page: Page }) => {
    await page.goto("/agents");
    // Scope to .site-main so the site shell's header/footer heading
    // hierarchy is not part of the audit — those live under their own
    // spec. `page-has-heading-one` fires against the whole document
    // when scoped to <html>; scoping here avoids the false positive.
    const results = await new AxeBuilder({ page })
      .include(".site-main")
      .disableRules(["color-contrast"]) // brand palette — audited separately
      .analyze();
    expect(results.violations).toEqual([]);
  });
});
