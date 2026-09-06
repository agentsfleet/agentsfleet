import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const FAQ_QUESTIONS = [
  "What does the Fleet read?",
  "What is agentsfleet?",
  "What does self-managed mean?",
  "What am I actually paying for?",
  "Does bringing my own model key make agentsfleet free?",
  "Can I self-host?",
  "Which coding agents work for the install skill?",
  "What if my Fleet hits the model's context window?",
] as const;

test.describe("touch navigation", () => {
  test.use({ hasTouch: true });

  for (const width of [320, 390]) {
    test(`primary destinations remain reachable above the fold at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 844 });
      await page.goto("/");
      expect(await page.evaluate(() => matchMedia("(pointer: coarse)").matches)).toBe(true);
      const nav = page.getByRole("navigation", { name: "Primary" });
      await expect(nav).toBeInViewport();
      for (const link of await nav.getByRole("link").all()) {
        await expect(link).toBeVisible();
        const bounds = await link.boundingBox();
        // Firefox reports 44px as 43.999992px; compare rendered pixel dimensions.
        expect(Math.round(bounds!.width)).toBeGreaterThanOrEqual(44);
        expect(Math.round(bounds!.height)).toBeGreaterThanOrEqual(44);
      }
      await nav.getByRole("link", { name: "agents", exact: true }).click();
      await expect(page.getByRole("heading", { level: 1 })).toHaveText("This page is for agents.");
      await expect(nav.getByRole("link", { name: "agents", exact: true })).toHaveAttribute("aria-current", "page");
      await nav.getByRole("link", { name: "how it works", exact: true }).click();
      const section = page.getByTestId("how-it-works");
      await expect(section).toBeInViewport();
      const sectionBounds = await section.boundingBox();
      const headerBounds = await page.getByRole("banner").boundingBox();
      expect(sectionBounds!.y).toBeGreaterThanOrEqual(headerBounds!.y + headerBounds!.height);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    });
  }
});

for (const theme of ["dark", "light"]) {
  for (const width of [1440, 390]) {
    test(`home content reflows at twice the text size in ${theme} at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/");
      await page.evaluate(value => {
        document.documentElement.setAttribute("data-theme", value);
        document.documentElement.style.fontSize = "200%";
      }, theme);
      await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
      const access = page.getByTestId("hero-cta-early-access");
      await access.scrollIntoViewIfNeeded();
      await expect(access).toBeInViewport();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
    });
  }

  for (const name of FAQ_QUESTIONS) {
    test(`FAQ “${name}” supports keyboard access in ${theme}`, async ({ page }) => {
      await page.setViewportSize({ width: 320, height: 900 });
      await page.emulateMedia({ reducedMotion: "reduce" });
      await page.goto("/");
      await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme);
      const question = page.getByTestId("faq").getByRole("button", { name, exact: true });
      await question.focus();
      await page.keyboard.press("Enter");
      await expect(question).toHaveAttribute("aria-expanded", "true");
      expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.keyboard.press("Enter");
      await expect(question).toHaveAttribute("aria-expanded", "false");
      await expect(question).toBeFocused();
    });
  }
}

for (const width of [1440, 320]) {
  test(`workflow examples support keyboard navigation at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/#how-it-works");
    const incident = page.getByRole("button", { name: "Incident Response", exact: true });
    await incident.focus();
    await page.keyboard.press("ArrowDown");
    const slack = page.getByRole("button", { name: "Slack Teammate", exact: true });
    await expect(slack).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(slack).toHaveAttribute("aria-expanded", "true");
    await expect(page.getByTestId("how-it-works")).toContainText("stays read-only");
    expect((await new AxeBuilder({ page }).analyze()).violations).toEqual([]);
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    await expect(incident).toHaveAttribute("aria-expanded", "true");
    await expect(page.getByTestId("how-it-works")).toContainText("You review and merge.");
  });
}
