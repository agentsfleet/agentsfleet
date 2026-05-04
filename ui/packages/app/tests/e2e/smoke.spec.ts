import { test, expect } from "@playwright/test";

const BASE_URL = process.env.BASE_URL ?? "http://localhost:3000";
const EXPECTED_HOSTNAME = new URL(BASE_URL).hostname;

test.describe("App smoke", () => {
  test("sign-in page loads", async ({ page }) => {
    await page.goto("/sign-in");
    await expect(page).toHaveURL(/\/sign-in/);
    await expect(page.getByText("usezombie", { exact: true })).toBeVisible();
  });

  test("protected route stays on first-party app surfaces", async ({ page }) => {
    await page.goto("/zombies");
    await page.waitForTimeout(1000);
    const redirected = new URL(page.url());
    expect(redirected.hostname).toBe(EXPECTED_HOSTNAME);
    expect(["/sign-in", "/zombies", "/"]).toContain(redirected.pathname);
    expect(page.url()).not.toContain("accounts.dev");
    await expect(page.locator("body")).toContainText(/usezombie|Zombies|Dashboard/);
  });
});
