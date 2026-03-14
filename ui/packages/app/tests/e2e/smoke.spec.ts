import { test, expect } from "@playwright/test";

test.describe("App smoke", () => {
  test("sign-in page loads", async ({ page }) => {
    await page.goto("/sign-in");
    await expect(page.getByText("UseZombie", { exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { level: 1, name: /sign in to my application/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /continue/i }).last()).toBeVisible();
  });

  test("protected route redirects to local sign-in", async ({ page }) => {
    await page.goto("/workspaces");
    await page.waitForTimeout(1000);
    expect(new URL(page.url()).hostname).toBe("localhost");
    expect(page.url()).not.toContain("accounts.dev");
    await expect(page.locator("body")).toContainText(/UseZombie|Workspaces/);
  });
});
