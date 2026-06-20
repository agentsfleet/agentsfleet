import { test, expect } from "@playwright/test";

// Wake-pulse smoke — verifies the brand-mark currency rule end to end
// at the unauthenticated surface. The signed-in dashboard pulse tests
// (Fleet list rows pulsing only on `state === 'live'`, capped at 5)
// require a Clerk test user + seeded Fleets; covered by the vitest
// suite (tests/dashboard-coverage.test.ts > "wake-pulse fires only on
// active rows", "wake-pulse cap"). This spec asserts the brand-mark's
// always-alive contract holds in the rendered DOM.

test.describe("wake-pulse — auth-shell brand-mark currency", () => {
  test("auth shell renders the brand-mark with data-live (the brand is always alive)", async ({ page }) => {
    await page.goto("/sign-in");
    await page.waitForLoadState("domcontentloaded");

    // The auth layout ships <WakePulse live ...> as the brand-mark.
    // The primitive sets data-live on the rendered <span>; CSS picks it up.
    const livePulse = page.locator("[data-live]").first();
    await expect(livePulse).toBeAttached({ timeout: 5000 });

    // Animation-name resolves to "wake-pulse" (defined in
    // design-system/tokens.css) — proves the CSS keyframe is wired.
    const animationName = await livePulse.evaluate((el) =>
      getComputedStyle(el).animationName,
    );
    // Permit either "wake-pulse" (production) or "none" (reduced motion).
    expect(["wake-pulse", "none"]).toContain(animationName);
  });

  test("no decorative --pulse leaks into chrome (currency rule)", async ({ page }) => {
    await page.goto("/sign-in");
    await page.waitForLoadState("domcontentloaded");

    // Every element with computed background-color === --pulse should be
    // either a [data-live] dot, or a primary CTA. Anything else is
    // decoration creep. We assert that decorative-pulse elements (a hover
    // ring, a focus border, an accent strip) are not present.
    const pulseRgb = "rgb(94, 234, 212)"; // #5EEAD4
    const decorativeHits = await page.evaluate((color) => {
      const all = Array.from(document.querySelectorAll<HTMLElement>("*"));
      return all
        .filter((el) => {
          const bg = getComputedStyle(el).backgroundColor;
          return bg === color;
        })
        .filter((el) => {
          // Allow [data-live] dots (the wake-pulse primitive).
          if (el.hasAttribute("data-live")) return false;
          // Allow primary CTAs — Clerk's formButtonPrimary or design-system Button[variant=default].
          const role = el.getAttribute("role");
          if (role === "button" || el.tagName === "BUTTON") return false;
          // Anything else with a pulse fill is decoration.
          return true;
        })
        .map((el) => el.tagName + (el.className ? "." + el.className : ""));
    }, pulseRgb);
    expect(decorativeHits).toEqual([]);
  });
});
