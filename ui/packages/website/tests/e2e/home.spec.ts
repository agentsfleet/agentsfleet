import { test, expect } from "@playwright/test";

test.describe("Home page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("renders hero heading", async ({ page }) => {
    const h1 = page.getByRole("heading", { level: 1 });
    await expect(h1).toContainText("Ship AI-generated PRs");
    await expect(h1).toContainText("without babysitting the run.");
  });

  test("renders hero CTAs", async ({ page }) => {
    const connect = page.getByRole("link", { name: /connect github, automate prs/i }).first();
    await expect(connect).toBeVisible();
    await expect(connect).toHaveAttribute("href", /app\.(dev\.)?usezombie\.com/);
    await expect(page.getByRole("link", { name: /talk to us/i })).toHaveCount(0);
  });

  test("renders bootstrap terminal command", async ({ page }) => {
    await expect(page.getByLabel(/quick start command/i)).toBeVisible();
    await expect(page.getByLabel(/quick start command/i)).toContainText("curl -fsSL https://usezombie.sh/install.sh | bash");
  });

  test("renders top header actions in humans mode", async ({ page }) => {
    await expect(page.locator("header").getByRole("link", { name: "Mission Control", exact: true })).toBeVisible();
    await expect(page.locator("header").getByRole("link", { name: "Connect GitHub, automate PRs" })).toHaveCount(0);
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

    const after = await mission.evaluate((el) => {
      const style = window.getComputedStyle(el);
      return {
        borderColor: style.borderColor,
        boxShadow: style.boxShadow,
      };
    });

    expect(after.borderColor).not.toBe(before.borderColor);
    expect(after.boxShadow).not.toBe("none");
  });

  test("renders feature flow rows", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Install once. Start shipping PRs." })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Traceability and replay by default" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Mission Control" })).toBeVisible();
  });

  test("renders how it works steps", async ({ page }) => {
    const how = page.locator(".how-steps");
    await expect(how.getByRole("heading", { name: "Queue work", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "Agents execute with guardrails", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "Review a validated PR", exact: true })).toBeVisible();
  });

  test("renders final install block actions", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 2, name: "Install zombiectl and connect GitHub" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Read the docs" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Connect GitHub, automate PRs" }).last()).toHaveAttribute(
      "href",
      /app\.(dev\.)?usezombie\.com/
    );
  });

  test("header Pricing link navigates to /pricing via React Router", async ({ page }) => {
    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Free and Scale plans");
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
