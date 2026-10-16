import { test, expect } from "@playwright/test";

// Branding stays static; actual fleet live-state coverage belongs to the
// dashboard tests. The wordmark must never imply a connected or running fleet.
test.describe("auth-shell shared identity", () => {
  test("uses the static signal mark under normal and reduced motion", async ({ page }) => {
    await page.goto("/sign-in");
    const mark = page.locator('[data-brand-mark="signal"]');
    await expect(mark).toBeVisible();
    await expect(mark).toHaveAttribute("aria-hidden", "true");
    await expect(mark).not.toHaveAttribute("data-live");
    await expect(mark).toHaveCSS("animation-name", "none");
    await page.emulateMedia({ reducedMotion: "reduce" });
    await expect(mark).toBeVisible();
    await expect(mark).toHaveCSS("animation-name", "none");
  });

  test("no decorative --pulse leaks into chrome (currency rule)", async ({ page }) => {
    await page.goto("/sign-in");
    await page.waitForLoadState("domcontentloaded");

    // Every element with computed background-color === --pulse should be
    // either a [data-live] dot, or a primary CTA. Anything else is
    // decoration creep. We assert that decorative-pulse elements (a hover
    // ring, a focus border, an accent strip) are not present.
    const decorativeHits = await page.evaluate(() => {
      const probe = document.createElement("span");
      probe.style.color = "var(--pulse)";
      document.body.appendChild(probe);
      const color = getComputedStyle(probe).color;
      probe.remove();
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
    });
    expect(decorativeHits).toEqual([]);
  });
});
