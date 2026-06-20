import { test, expect } from "@playwright/test";

/**
 * Navigation edge cases. The Humans/Fleets mode-switch tab pill was
 * removed in M64_003 (Mockup A simplified topbar), so all related
 * tests are dropped — both routes still exist as regular nav links.
 */

test.describe("Footer navigation", () => {
  test("footer fleets link navigates to /fleets", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: /^fleets$/i }).click();
    await expect(page).toHaveURL(/\/fleets/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous Fleets.",
    );
  });

  test("footer pricing link navigates to home pricing anchor", async ({ page }) => {
    await page.goto("/fleets");
    await page.getByRole("contentinfo").getByRole("link", { name: /^pricing$/i }).click();
    await expect(page).toHaveURL(/\/#pricing$/);
    await expect(page.getByTestId("pricing-block")).toBeVisible();
  });

  test("footer fleet link navigates to the prebuilt-fleets anchor", async ({ page }) => {
    await page.goto("/fleets");
    await page.getByRole("contentinfo").getByRole("link", { name: /^fleet$/i }).click();
    await expect(page).toHaveURL(/\/#operational-loop$/);
    await expect(page.getByTestId("prebuilt-fleets")).toBeVisible();
  });

  test("footer privacy link navigates to /privacy", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: /^privacy$/i }).click();
    await expect(page).toHaveURL(/\/privacy/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Privacy Policy");
  });

  test("footer terms link navigates to /terms", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: /^terms$/i }).click();
    await expect(page).toHaveURL(/\/terms/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Terms of Service");
  });

  test("footer Discord link has canonical URL and opens in new tab", async ({ page }) => {
    await page.goto("/");
    const discord = page.getByRole("contentinfo").getByRole("link", { name: /^discord$/i });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
    await expect(discord).toHaveAttribute("target", "_blank");
    await expect(discord).toHaveAttribute("rel", "noopener noreferrer");
  });

  test("footer GitHub link opens in new tab", async ({ page }) => {
    await page.goto("/");
    const github = page.getByRole("contentinfo").getByRole("link", { name: /^github$/i });
    await expect(github).toHaveAttribute("target", "_blank");
    await expect(github).toHaveAttribute("rel", "noopener noreferrer");
  });
});

test.describe("Direct URL navigation", () => {
  test("direct nav to /fleets renders the fleets heading", async ({ page }) => {
    await page.goto("/fleets");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous Fleets.",
    );
  });

  test("direct nav to /#pricing scrolls to inline pricing section", async ({ page }) => {
    await page.goto("/#pricing");
    await expect(page.getByTestId("pricing-block")).toBeVisible();
  });

  test("direct nav to /privacy renders the privacy heading", async ({ page }) => {
    await page.goto("/privacy");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Privacy Policy");
  });

  test("direct nav to /terms renders the terms heading", async ({ page }) => {
    await page.goto("/terms");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Terms of Service");
  });
});

test.describe("SPA routing — no full page reloads", () => {
  test("topbar pricing anchor scrolls to inline section", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: /^pricing$/i }).click();
    await expect(page).toHaveURL(/\/#pricing$/);
    await expect(page.getByTestId("pricing-block")).toBeVisible();
  });

  test("footer fleets link is a real anchor (React Router Link)", async ({ page }) => {
    await page.goto("/");
    const agentsLink = page.getByRole("contentinfo").getByRole("link", { name: /^fleets$/i });
    await expect(agentsLink).toHaveAttribute("href", "/fleets");
  });

  test("footer pricing link points at home anchor", async ({ page }) => {
    await page.goto("/");
    const pricingLink = page.getByRole("contentinfo").getByRole("link", { name: /^pricing$/i });
    await expect(pricingLink).toHaveAttribute("href", "/#pricing");
  });
});

test.describe("Fleets page — install block", () => {
  test("install agentsfleet block is visible on /fleets", async ({ page }) => {
    await page.goto("/fleets");
    await expect(page.getByRole("heading", { name: /install agentsfleet/i })).toBeVisible();
  });

  test("npm install command is readable in install block", async ({ page }) => {
    await page.goto("/fleets");
    const block = page.getByLabel(/bootstrap commands/i);
    await expect(block).toContainText("npm install -g @agentsfleet/cli");
  });

  test("read the docs button links to docs", async ({ page }) => {
    await page.goto("/fleets");
    await expect(page.getByRole("link", { name: /read the docs/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
  });

  test("start a Fleet button is visible; no dashboard link", async ({ page }) => {
    await page.goto("/fleets");
    await expect(page.getByRole("link", { name: /start a Fleet/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /open dashboard/i })).toHaveCount(0);
  });
});
