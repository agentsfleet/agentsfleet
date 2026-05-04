import { test, expect } from "@playwright/test";

test.describe("Home page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("renders hero heading", async ({ page }) => {
    const h1 = page.getByRole("heading", { level: 1 });
    await expect(h1).toContainText("Operational outcomes");
    await expect(h1).toContainText("don't fall into limbo.");
  });

  test("renders hero CTAs", async ({ page }) => {
    const install = page.getByRole("link", { name: /install platform-ops/i }).first();
    await expect(install).toBeVisible();
    await expect(install).toHaveAttribute("href", /docs\.usezombie\.com\/quickstart/);
    await expect(page.getByRole("link", { name: /talk to us/i })).toHaveCount(0);
  });

  test("renders bootstrap terminal command", async ({ page }) => {
    await expect(page.getByLabel(/quick start command/i)).toBeVisible();
    await expect(page.getByLabel(/quick start command/i)).toContainText("npm install -g @usezombie/zombiectl");
  });

  test("renders top header actions in humans mode", async ({ page }) => {
    await expect(page.locator("header").getByRole("link", { name: "Mission Control", exact: true })).toBeVisible();
    await expect(page.locator("header").getByRole("link", { name: "Install platform-ops" })).toHaveCount(0);
  });

  test("Mission Control button applies hover style", async ({ page }) => {
    const mission = page.locator("header").getByRole("link", { name: "Mission Control", exact: true });
    await expect(mission).toBeVisible();

    const before = await mission.evaluate((el) => {
      const style = window.getComputedStyle(el);
      return {
        borderColor: style.borderColor,
        boxShadow: style.boxShadow,
      };
    });

    await mission.hover();

    // The ghost Button variant transitions border-color/box-shadow over
    // ~150ms (see Button.tsx `transition-[...] ease-fast`). Reading
    // computed style immediately after hover() catches the interpolation
    // mid-flight and yields the pre-hover value. Poll until the transition
    // has settled on the actual hover state.
    await expect
      .poll(
        async () =>
          mission.evaluate((el) => {
            const style = window.getComputedStyle(el);
            return {
              borderColor: style.borderColor,
              boxShadow: style.boxShadow,
            };
          }),
        { timeout: 2000, intervals: [50, 100, 200] },
      )
      .toMatchObject({
        borderColor: expect.not.stringMatching(
          new RegExp(`^${before.borderColor.replace(/[()]/g, "\\$&")}$`),
        ),
        boxShadow: expect.not.stringMatching(/^none$/),
      });
  });

  test("renders feature flow rows", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Install once. Operate forever." })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Every event, every actor, on the record." })).toBeVisible();
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
    await expect(page.getByRole("link", { name: "Install platform-ops" }).last()).toHaveAttribute(
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
