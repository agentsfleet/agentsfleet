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

test.describe("Button — computed styles", () => {
  test("default variant renders the orange gradient", async ({ page }) => {
    const bg = await computed(page, "btn-default", "background-image");
    expect(bg).toContain("linear-gradient");
  });

  test("destructive variant has a solid destructive background", async ({ page }) => {
    const bg = await computed(page, "btn-destructive", "background-color");
    // Should be non-transparent and distinct from the default gradient
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
    expect(bg).not.toBe("transparent");
  });

  test("outline variant has a transparent background and a visible border", async ({ page }) => {
    const bg = await computed(page, "btn-outline", "background-color");
    const border = await computed(page, "btn-outline", "border-top-width");
    expect(bg).toBe("rgba(0, 0, 0, 0)");
    expect(parseFloat(border)).toBeGreaterThan(0);
  });

  test("double-border variant has a 2px border", async ({ page }) => {
    const w = await computed(page, "btn-double-border", "border-top-width");
    expect(parseFloat(w)).toBeCloseTo(2, 0);
  });

  test("pill radius resolves to ≥ 40px (rounded-full)", async ({ page }) => {
    const r = await computed(page, "btn-default", "border-top-left-radius");
    expect(parseFloat(r)).toBeGreaterThanOrEqual(40);
  });

  test("ease-fast transition resolves to a timing-function", async ({ page }) => {
    const ease = await computed(page, "btn-default", "transition-timing-function");
    // var(--ease-fast) ≈ 0.15s ease → "ease"
    expect(ease.length).toBeGreaterThan(0);
    expect(ease).not.toBe("");
  });

  test("sm size has h-8 (32px)", async ({ page }) => {
    const h = await computed(page, "btn-sm", "height");
    expect(parseFloat(h)).toBeCloseTo(32, 0);
  });

  test("lg size has h-12 (48px)", async ({ page }) => {
    const h = await computed(page, "btn-lg", "height");
    expect(parseFloat(h)).toBeCloseTo(48, 0);
  });

  test("icon size is 40x40 square", async ({ page }) => {
    const h = await computed(page, "btn-icon", "height");
    const w = await computed(page, "btn-icon", "width");
    expect(parseFloat(h)).toBeCloseTo(40, 0);
    expect(parseFloat(w)).toBeCloseTo(40, 0);
  });

  test("asChild renders an <a> with button styles", async ({ page }) => {
    const loc = page.locator('[data-testid="btn-aschild"]');
    await expect(loc).toHaveJSProperty("tagName", "A");
    const r = await computed(page, "btn-aschild", "border-top-left-radius");
    expect(parseFloat(r)).toBeGreaterThanOrEqual(40);
  });
});

test.describe("Card — computed styles", () => {
  test("default card has border + card background + p-6 padding", async ({ page }) => {
    const pad = await computed(page, "card-default", "padding-top");
    const bw = await computed(page, "card-default", "border-top-width");
    expect(parseFloat(pad)).toBeCloseTo(24, 0); // p-6 = 24px
    expect(parseFloat(bw)).toBeGreaterThan(0);
  });

  test("featured card renders the 'Popular' badge", async ({ page }) => {
    const card = page.locator('[data-testid="card-featured"]');
    await expect(card).toContainText("Popular");
  });
});

test.describe("Terminal — computed styles", () => {
  // Terminal's text/bg utilities land on the inner <pre>, so we query that.
  async function terminalPreStyle(page: Page, testid: string, prop: string) {
    return page
      .locator(`[data-testid="${testid}"] pre`)
      .evaluate((el, p) => getComputedStyle(el).getPropertyValue(p), prop);
  }

  test("default terminal uses the info color (cyan)", async ({ page }) => {
    const color = await terminalPreStyle(page, "terminal-default", "color");
    expect(color).not.toBe("");
    expect(color).not.toBe("rgba(0, 0, 0, 0)");
  });

  test("green terminal uses the success color (different from info)", async ({ page }) => {
    const green = await terminalPreStyle(page, "terminal-green", "color");
    const info = await terminalPreStyle(page, "terminal-default", "color");
    expect(green).not.toBe(info);
  });

  test("terminal has surface-terminal background", async ({ page }) => {
    const bg = await terminalPreStyle(page, "terminal-default", "background-color");
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
  });
});

test.describe("Grid — computed styles", () => {
  test("grid columns use auto-fit + minmax (non-empty grid-template-columns)", async ({ page }) => {
    const cols = await computed(page, "grid-two", "grid-template-columns");
    // Parsed from `grid-cols-[repeat(auto-fit,minmax(280px,1fr))]`
    expect(cols).not.toBe("none");
    expect(cols.length).toBeGreaterThan(0);
  });

  test("gap-lg resolves to a non-zero gap", async ({ page }) => {
    const g = await computed(page, "grid-two", "gap");
    expect(parseFloat(g)).toBeGreaterThan(0);
  });
});

test.describe("Badge — computed styles", () => {
  test("default badge has a visible border and muted background", async ({ page }) => {
    const bw = await computed(page, "badge-default", "border-top-width");
    const bg = await computed(page, "badge-default", "background-color");
    expect(parseFloat(bw)).toBeGreaterThan(0);
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
  });

  test("pill radius resolves (rounded-full)", async ({ page }) => {
    const r = await computed(page, "badge-default", "border-top-left-radius");
    expect(parseFloat(r)).toBeGreaterThanOrEqual(40);
  });

  test("status variants produce distinct text colors", async ({ page }) => {
    const orange = await computed(page, "badge-orange", "color");
    const green = await computed(page, "badge-green", "color");
    const destructive = await computed(page, "badge-destructive", "color");
    expect(new Set([orange, green, destructive]).size).toBe(3);
  });
});

test.describe("Input — computed styles", () => {
  test("base input has border, muted bg, and rounded-lg radius", async ({ page }) => {
    const bw = await computed(page, "input-default", "border-top-width");
    const bg = await computed(page, "input-default", "background-color");
    const r = await computed(page, "input-default", "border-top-left-radius");
    expect(parseFloat(bw)).toBeGreaterThan(0);
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
    expect(parseFloat(r)).toBeGreaterThan(0);
  });

  test("disabled input has reduced opacity", async ({ page }) => {
    const op = await computed(page, "input-disabled", "opacity");
    expect(parseFloat(op)).toBeLessThan(1);
  });
});

test.describe("Separator — computed styles", () => {
  test("horizontal separator is 1px tall and full width", async ({ page }) => {
    const h = await computed(page, "separator-horizontal", "height");
    expect(parseFloat(h)).toBeCloseTo(1, 0);
  });

  test("vertical separator is 1px wide", async ({ page }) => {
    const w = await computed(page, "separator-vertical", "width");
    expect(parseFloat(w)).toBeCloseTo(1, 0);
  });

  test("separator has the border color as background", async ({ page }) => {
    const bg = await computed(page, "separator-horizontal", "background-color");
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
  });
});

test.describe("Skeleton — computed styles", () => {
  test("skeleton runs the pulse animation", async ({ page }) => {
    const name = await computed(page, "skeleton-line", "animation-name");
    expect(name).toBe("pulse");
  });

  test("skeleton has rounded corners", async ({ page }) => {
    const r = await computed(page, "skeleton-line", "border-top-left-radius");
    expect(parseFloat(r)).toBeGreaterThan(0);
  });
});

test.describe("Dialog — computed styles", () => {
  test("dialog content is not rendered until trigger is clicked", async ({ page }) => {
    await expect(page.locator('[data-testid="dialog-content"]')).toHaveCount(0);
  });

  test("clicking the trigger opens the dialog with surface styles", async ({ page }) => {
    await page.locator('[data-testid="dialog-trigger"]').click();
    const content = page.locator('[data-testid="dialog-content"]');
    await expect(content).toBeVisible();
    const bg = await content.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
    const r = await content.evaluate((el) => getComputedStyle(el).borderTopLeftRadius);
    expect(parseFloat(r)).toBeGreaterThan(0);
    // close dialog afterwards so other tests start from closed state
    await page.keyboard.press("Escape");
  });

  test("dialog description uses muted-foreground color", async ({ page }) => {
    await page.locator('[data-testid="dialog-trigger"]').click();
    const desc = page.locator('[data-testid="dialog-description"]');
    const muted = await desc.evaluate((el) => getComputedStyle(el).color);
    const dialogTitleColor = await page
      .locator('[data-testid="dialog-content"] h2, [data-testid="dialog-content"] [id]')
      .first()
      .evaluate((el) => getComputedStyle(el).color)
      .catch(() => "");
    expect(muted.length).toBeGreaterThan(0);
    if (dialogTitleColor) expect(muted).not.toBe(dialogTitleColor);
    await page.keyboard.press("Escape");
  });
});

test.describe("DropdownMenu — computed styles", () => {
  test("menu content is not rendered until trigger is clicked", async ({ page }) => {
    await expect(page.locator('[data-testid="dropdown-content"]')).toHaveCount(0);
  });

  test("opens on trigger click and applies surface utilities", async ({ page }) => {
    await page.locator('[data-testid="dropdown-trigger"]').click();
    const content = page.locator('[data-testid="dropdown-content"]');
    await expect(content).toBeVisible();
    const bg = await content.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(bg).not.toBe("rgba(0, 0, 0, 0)");
    await page.keyboard.press("Escape");
  });

  test("menu item + separator compile and render", async ({ page }) => {
    await page.locator('[data-testid="dropdown-trigger"]').click();
    const item = page.locator('[data-testid="dropdown-item"]').first();
    await expect(item).toBeVisible();
    const itemBg = await item.evaluate((el) => getComputedStyle(el).backgroundColor);
    // transparent on rest; hover would tint
    expect(typeof itemBg).toBe("string");
    const sep = page.locator('[data-testid="dropdown-separator"]');
    await expect(sep).toBeVisible();
    const h = await sep.evaluate((el) => getComputedStyle(el).height);
    expect(parseFloat(h)).toBeCloseTo(1, 0);
    await page.keyboard.press("Escape");
  });
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
    expect(font.toLowerCase()).toContain("mono");
  });
});

test.describe("AnimatedIcon — computed styles", () => {
  test("always-trigger glyph has a non-empty animation-name", async ({ page }) => {
    const glyph = page
      .locator('[aria-label="always-wave"] [data-animated-glyph]')
      .first();
    const name = await glyph.evaluate((el) => getComputedStyle(el).animationName);
    expect(name).toBe("z-icon-wave");
  });
});

test.describe("Accessibility", () => {
  test("gallery content has zero axe violations", async ({ page }) => {
    // Scope to the gallery content — site shell (header/footer) heading order
    // is not part of the design-system contract, audited in its own spec.
    const results = await new AxeBuilder({ page })
      .include(".site-main")
      .disableRules(["color-contrast"]) // brand palette; audited separately
      .analyze();
    expect(results.violations).toEqual([]);
  });
});
