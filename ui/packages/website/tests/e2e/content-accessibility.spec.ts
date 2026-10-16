import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
const SCREENSHOT_HEIGHT = 1000;
const BROWSER_TIMEOUT_DEFAULT = "Number(process.env.BROWSER_TIMEOUT) || 5";

for (const width of [320, 375, 768, 1440]) {
  test(`conversation and evidence work with keyboard at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/#how-it-works");
    await expect(page.getByRole("textbox")).toHaveCount(0);
    if (width < 768) {
      const titleWidth = await page.locator(".workbench-bar h2").evaluate(element => element.getBoundingClientRect().width);
      const rowWidth = await page.locator(".workbench-bar").evaluate(element => {
        const style = getComputedStyle(element);
        return element.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      });
      expect(titleWidth).toBeCloseTo(rowWidth);
    }
    await page.keyboard.press("Tab");
    await expect(page.getByRole("link", { name: "Skip to content" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.locator("main")).toBeFocused();
    const evidence = page.getByRole("button", { name: "E2 · GitHub" });
    await evidence.focus(); await page.keyboard.press("Enter");
    await expect(page.getByRole("region", { name: "Sample evidence" })).toContainText(BROWSER_TIMEOUT_DEFAULT);
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
    await page.getByRole("button", { name: "Close evidence" }).click();
    await page.getByRole("link", { name: "Create fleet", exact: true }).focus();
    await expect(page.getByRole("link", { name: "Create fleet", exact: true })).toBeFocused();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}
for (const width of [390, 1440]) {
  test(`text zoom and reduced motion retain access at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");
    await page.evaluate(() => { document.documentElement.style.fontSize = "200%"; });
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.getByRole("link", { name: /Automate recurring work/ }).scrollIntoViewIfNeeded();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await expect(page.locator(".signal-field > g")).toHaveCSS("animation-name", "none");
    await page.getByRole("link", { name: /Investigate an incident.*Explore example run/ }).click();
    await expect(page.getByRole("textbox")).toHaveCount(0);
    await expect(page.getByTestId("how-it-works")).toHaveAttribute("data-running", "false");
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
  });
}
test("captures desktop, tablet, and mobile evidence", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  for (const [name, width] of [["desktop", 1440], ["tablet", 768], ["mobile", 375]] as const) {
    await page.setViewportSize({ width, height: SCREENSHOT_HEIGHT });
    await page.goto("/");
    await page.evaluate(async () => { await document.fonts.ready; window.scrollTo(0, 0); });
    await page.screenshot({ path: `/tmp/agentsfleet-website-${name}.png`, fullPage: true });
  }
});
