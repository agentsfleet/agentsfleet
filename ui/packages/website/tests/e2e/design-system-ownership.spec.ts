import { expect, test } from "@playwright/test";

for (const theme of ["dark", "light"]) {
  test(`shared app primitives render independently in ${theme}`, async ({ page }) => {
    await page.goto("/_design-system");
    await page.evaluate((value) => document.documentElement.dataset.theme = value, theme);
    const section = page.getByTestId("app-primitives");
    await expect(section).toBeVisible();
    const active = section.getByRole("link", { name: "Fleets" });
    await expect(active).toHaveAttribute("aria-current", "page");
    expect(await active.evaluate((el) => getComputedStyle(el).fontFamily)).toContain("Instrument Sans");
    const fill = section.locator(".usage-bar-fill");
    expect(await fill.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe("rgba(0, 0, 0, 0)");
    expect(await fill.evaluate((el) => el.getBoundingClientRect().width)).toBeGreaterThan(0);
  });
}

test("changing shared tokens updates consumers while technical text keeps its role", async ({ page }) => {
  await page.goto("/_design-system");
  const section = page.getByTestId("app-primitives");
  const technicalFont = await section.locator("code").evaluate((el) => getComputedStyle(el).fontFamily);
  await page.evaluate(() => {
    document.documentElement.style.setProperty("--ff-sans", "Georgia");
    document.documentElement.style.setProperty("--pulse", "rgb(11, 22, 33)");
  });
  for (const locator of [section.getByRole("link").first(), section.getByRole("heading"), page.getByTestId("btn-default")]) {
    expect(await locator.evaluate((el) => getComputedStyle(el).fontFamily)).toContain("Georgia");
  }
  expect(await section.locator(".usage-bar-fill").evaluate((el) => getComputedStyle(el).backgroundColor)).toBe("rgb(11, 22, 33)");
  expect(await section.locator("code").evaluate((el) => getComputedStyle(el).fontFamily)).toBe(technicalFont);
});
