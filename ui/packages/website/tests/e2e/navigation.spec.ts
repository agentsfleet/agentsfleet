import { test, expect } from "@playwright/test";

/** Footer links reach current pages; removed routes show the recovery page. */

test.describe("Footer navigation", () => {
  test("footer agents link navigates to /agents", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: /^Agents$/i }).click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for agents.",
    );
  });

  test("footer access link matches the waitlist destination", async ({ page }) => {
    await page.goto("/agents");
    const header = page.getByTestId("header-install-cta");
    await expect(header).toHaveText("Join waitlist ↗");
    const href = await header.getAttribute("href");
    expect(href).toMatch(/\/waitlist$/);
    await expect(page.getByRole("contentinfo").getByRole("link", { name: "join waitlist", exact: true })).toHaveAttribute("href", href!);
  });

  test("footer use cases link opens the example conversation", async ({ page }) => {
    await page.goto("/agents");
    await page.getByRole("contentinfo").getByRole("link", { name: /^Use cases$/i }).click();
    await expect(page).toHaveURL(/\/#how-it-works$/);
    await expect(page.getByRole("figure")).toBeVisible();
    await expect(page.getByRole("textbox", { name: "What should your fleet handle?" })).toHaveCount(0);
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

  test("footer GitHub link opens in new tab", async ({ page }) => {
    await page.goto("/");
    const github = page.getByRole("contentinfo").getByRole("link", { name: /^github$/i });
    await expect(github).toHaveAttribute("target", "_blank");
    await expect(github).toHaveAttribute("rel", "noopener noreferrer");
  });
});

test("removed page URLs show not-found with working recovery links", async ({ page }) => {
  for (const path of ["/fleets", "/pricing"]) {
    await page.goto(path);
    await expect(page).toHaveURL(path);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("This page isn’t here.");
    await page.getByRole("link", { name: "Explore agents", exact: true }).click();
    await expect(page).toHaveURL(/\/agents$/);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("This page is for agents.");
  }
});
