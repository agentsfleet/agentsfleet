import { test, expect } from "@playwright/test";

test.describe("Home page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("renders hero heading", async ({ page }) => {
    const h1 = page.getByRole("heading", { level: 1 });
    await expect(h1).toContainText("Agents that wake on every event.");
  });

  test("renders hero CTAs", async ({ page }) => {
    const install = page.getByRole("link", { name: /start an agent/i }).first();
    await expect(install).toBeVisible();
    await expect(install).toHaveAttribute("href", /docs\.usezombie\.com\/quickstart/);
    await expect(page.getByRole("link", { name: /talk to us/i })).toHaveCount(0);
    // Offer-led secondary CTA pulls toward the $5 starter credit on /pricing.
    await expect(page.getByRole("link", { name: /\$5 starter credit/i }).first()).toHaveAttribute(
      "href",
      /\/pricing/,
    );
  });

  test("renders bootstrap terminal command", async ({ page }) => {
    await expect(page.getByLabel(/quick start command/i)).toBeVisible();
    await expect(page.getByLabel(/quick start command/i)).toContainText("npm install -g @usezombie/zombiectl");
  });

  test("renders top header actions in humans mode", async ({ page }) => {
    await expect(page.locator("header").getByRole("link", { name: "Try usezombie", exact: true })).toBeVisible();
    await expect(page.locator("header").getByRole("link", { name: /start an agent/i })).toHaveCount(0);
  });

  test("Try usezombie button is a gradient pill that lifts on hover", async ({ page }) => {
    const mission = page.locator("header").getByRole("link", { name: "Try usezombie", exact: true });
    await expect(mission).toBeVisible();

    // The CTA is the only fully-coloured element in the header strip — the
    // brand orange→cyan gradient is the visual anchor.
    const baseBackground = await mission.evaluate(
      (el) => window.getComputedStyle(el).backgroundImage,
    );
    expect(baseBackground).toMatch(/linear-gradient/);

    const before = await mission.evaluate((el) => {
      const style = window.getComputedStyle(el);
      return { boxShadow: style.boxShadow, transform: style.transform };
    });

    await mission.hover();

    // Hover deepens the shadow and lifts by 1px. Gradient + brightness filter
    // also shift, but transitions in jsdom-less browsers settle within ~250ms
    // — poll until the box-shadow has changed from the resting state.
    await expect
      .poll(
        async () =>
          mission.evaluate((el) => {
            const style = window.getComputedStyle(el);
            return { boxShadow: style.boxShadow, transform: style.transform };
          }),
        { timeout: 2000, intervals: [50, 100, 200] },
      )
      .toMatchObject({
        boxShadow: expect.not.stringMatching(
          new RegExp(`^${before.boxShadow.replace(/[()]/g, "\\$&")}$`),
        ),
        transform: expect.not.stringMatching(/^none$/),
      });
  });

  test("Try usezombie zombie-hand icon fades in on hover", async ({ page }) => {
    const mission = page.locator("header").getByRole("link", { name: "Try usezombie", exact: true });
    const iconSlot = mission.locator(".header-mission-control-icon");

    // Resting: opacity 0 — the hand is hidden until the user telegraphs
    // intent. Reading immediately is fine because the resting state has
    // no transition delay.
    const restingOpacity = await iconSlot.evaluate(
      (el) => Number(window.getComputedStyle(el).opacity),
    );
    expect(restingOpacity).toBeLessThan(0.1);

    await mission.hover();

    // Hover: the slot fades to opacity 1 and the inner glyph plays the
    // `animate-drop` keyframe (translateY -16 → 0). Poll until the slot
    // opacity has settled.
    await expect
      .poll(
        async () =>
          iconSlot.evaluate((el) => Number(window.getComputedStyle(el).opacity)),
        { timeout: 2000, intervals: [50, 100, 200] },
      )
      .toBeGreaterThan(0.9);
  });

  test("renders feature flow rows", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Install once." })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Every event on the record." })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Mission Control" })).toBeVisible();
  });

  test("renders how it works steps", async ({ page }) => {
    const how = page.locator(".how-steps");
    await expect(how.getByRole("heading", { name: "A trigger arrives", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "The agent gathers evidence", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "Diagnosis posts; the run is auditable", exact: true })).toBeVisible();
  });

  test("renders final install block actions", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 2, name: "Install zombiectl, then run /usezombie-install-platform-ops" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Read the docs" })).toBeVisible();
    await expect(page.getByRole("link", { name: /start an agent/i }).last()).toHaveAttribute(
      "href",
      /docs\.usezombie\.com\/quickstart/
    );
  });

  test("header Pricing link navigates to /pricing via React Router", async ({ page }) => {
    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Start free. Upgrade when you need stronger control.");
  });

  test("footer is present with canonical Discord URL", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
    const footer = page.getByRole("contentinfo");
    await expect(footer.getByRole("link", { name: "GitHub" })).toBeVisible();
    const discord = footer.getByRole("link", { name: "Discord" });
    await expect(discord).toBeVisible();
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
  });
});
