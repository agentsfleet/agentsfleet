import { test, expect } from "@playwright/test";

/** Footer links and old URLs reach the pages they name. */

test.describe("Footer navigation", () => {
  test("footer fleets link navigates to /fleets", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("contentinfo").getByRole("link", { name: /^Agents$/i }).click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for agents.",
    );
  });

  test("footer Dashboard matches the header waitlist destination", async ({ page }) => {
    await page.goto("/agents");
    const header = page.getByTestId("header-install-cta");
    await expect(header).toHaveText("dashboard");
    const href = await header.getAttribute("href");
    expect(href).toMatch(/\/waitlist$/);
    await expect(page.getByRole("contentinfo").getByRole("link", { name: "dashboard", exact: true })).toHaveAttribute("href", href!);
  });

  test("footer fleet link navigates to the prebuilt-fleets anchor", async ({ page }) => {
    await page.goto("/agents");
    await page.getByRole("contentinfo").getByRole("link", { name: /^Use cases$/i }).click();
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

  test("footer GitHub link opens in new tab", async ({ page }) => {
    await page.goto("/");
    const github = page.getByRole("contentinfo").getByRole("link", { name: /^github$/i });
    await expect(github).toHaveAttribute("target", "_blank");
    await expect(github).toHaveAttribute("rel", "noopener noreferrer");
  });
});

test("old page URLs lead to their current destinations", async ({ page }) => {
  await page.goto("/fleets");
  await expect(page).toHaveURL(/\/agents$/);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("This page is for agents.");
  await page.goto("/pricing");
  await expect(page).toHaveURL(/\/#pricing$/);
  await expect(page.getByTestId("pricing-block")).toBeVisible();
});
