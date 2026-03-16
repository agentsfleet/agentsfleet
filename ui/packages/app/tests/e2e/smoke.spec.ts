import { test, expect } from "@playwright/test";

const BASE_URL = process.env.BASE_URL ?? "http://localhost:3000";
const EXPECTED_HOSTNAME = new URL(BASE_URL).hostname;

test.describe("App smoke", () => {
  test("sign-in page loads", async ({ page }) => {
    await page.goto("/sign-in");
    await expect(page.getByText("UseZombie", { exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 1, name: /sign in/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /continue/i }).last()).toBeVisible();
  });

  test("protected route redirects to local sign-in", async ({ page }) => {
    await page.goto("/workspaces");
    await page.waitForTimeout(1000);
    const redirected = new URL(page.url());
    expect(redirected.hostname).toBe(EXPECTED_HOSTNAME);
    expect(redirected.pathname).toContain("/sign-in");
    expect(page.url()).not.toContain("accounts.dev");
    await expect(page.locator("body")).toContainText(/UseZombie|Workspaces/);
  });
});
