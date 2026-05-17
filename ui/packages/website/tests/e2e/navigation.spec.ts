import { test, expect } from "@playwright/test";

/**
 * Navigation edge cases. The Humans/Agents mode-switch tab pill was
 * removed in M64_003 (Mockup A simplified topbar), so all related
 * tests are dropped — both routes still exist as regular nav links.
 */

test.describe("Footer navigation", () => {
  test("footer agents link navigates to /agents", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: /^agents$/i }).click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous agents.",
    );
  });

  test("footer pricing link navigates to home pricing anchor", async ({ page }) => {
    await page.goto("/agents");
    await page.getByRole("contentinfo").getByRole("link", { name: /^pricing$/i }).click();
    await expect(page).toHaveURL(/\/#pricing$/);
    await expect(page.getByTestId("pricing-block")).toBeVisible();
  });

  test("footer features link navigates to /", async ({ page }) => {
    await page.goto("/agents");
    await page.getByRole("contentinfo").getByRole("link", { name: /^features$/i }).click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
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
  test("direct nav to /agents renders the agents heading", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous agents.",
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

  test("footer agents link is a real anchor (React Router Link)", async ({ page }) => {
    await page.goto("/");
    const agentsLink = page.getByRole("contentinfo").getByRole("link", { name: /^agents$/i });
    await expect(agentsLink).toHaveAttribute("href", "/agents");
  });

  test("footer pricing link points at home anchor", async ({ page }) => {
    await page.goto("/");
    const pricingLink = page.getByRole("contentinfo").getByRole("link", { name: /^pricing$/i });
    await expect(pricingLink).toHaveAttribute("href", "/#pricing");
  });
});

test.describe("Agents page — install block", () => {
  test("install zombiectl block is visible on /agents", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { name: /install zombiectl/i })).toBeVisible();
  });

  test("npm install command is readable in install block", async ({ page }) => {
    await page.goto("/agents");
    const block = page.getByLabel(/install zombiectl command/i);
    await expect(block).toContainText("npm install -g @usezombie/zombiectl");
  });

  test("read the docs button links to docs", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("link", { name: /read the docs/i })).toHaveAttribute(
      "href",
      "https://docs.usezombie.com",
    );
  });

  test("open dashboard button is visible", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("link", { name: /open dashboard/i })).toBeVisible();
  });
});
