import { test, expect } from "@playwright/test";

test.describe("Agents page (/agents)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/agents");
  });

  test("renders safety limits as a constraint table", async ({ page }) => {
    await expect(page.getByRole("rowheader", { name: /^idempotency$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^audit trail$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^secret management$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^policy enforcement$/i })).toBeVisible();
  });

  test("footer renders on fleets page", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
  });
});
