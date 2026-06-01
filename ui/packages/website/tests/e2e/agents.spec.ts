import { test, expect } from "@playwright/test";

test.describe("Agents page (/agents)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/agents");
  });

  test("renders agent-first heading", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous agents.",
    );
  });

  test("renders install block with npm command", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /install zombiectl/i })).toBeVisible();
    await expect(page.getByLabel(/install zombiectl command/i)).toContainText(
      "npm install -g @usezombie/zombiectl",
    );
  });

  test("renders install block action buttons (lowercase)", async ({ page }) => {
    await expect(page.getByRole("link", { name: /start an agent/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /read the docs/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /open dashboard/i })).toBeVisible();
  });

  test("renders bootstrap commands", async ({ page }) => {
    const block = page.getByLabel(/bootstrap commands/i);
    await expect(block).toBeVisible();
    await expect(block).toContainText("npm install -g @usezombie/zombiectl");
    await expect(block).toContainText("zombiectl login");
    await expect(block).toContainText("npx skills add usezombie/skills");
    await expect(block).toContainText("/usezombie-install-platform-ops");
  });

  test("renders machine surface heading + openapi link", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /machine surface/i })).toBeVisible();
    await expect(page.getByTestId("agents-openapi-link")).toHaveAttribute("href", "/openapi.json");
  });

  test("renders API operations table", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /api operations/i })).toBeVisible();
    await expect(page.getByText("Create agent")).toBeVisible();
    await expect(page.getByText("Stop agent")).toBeVisible();
    await expect(page.getByText("Resume agent")).toBeVisible();
    await expect(page.getByText("Kill agent")).toBeVisible();
    await expect(page.getByText("Delete agent")).toBeVisible();
    await expect(page.getByText("Steer / chat")).toBeVisible();
  });

  test("renders webhook example", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /webhook ingest example/i })).toBeVisible();
    await expect(page.getByText(/deploy\.failed/)).toBeVisible();
  });

  test("renders safety limit cards (sentence-case)", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /^idempotency$/i })).toBeVisible();
    await expect(page.getByRole("heading", { name: /^audit trail$/i })).toBeVisible();
    await expect(page.getByRole("heading", { name: /^secret management$/i })).toBeVisible();
    await expect(page.getByRole("heading", { name: /^policy enforcement$/i })).toBeVisible();
  });

  test("does not render orange-era decorative chrome", async ({ page }) => {
    await expect(page.locator(".scanline")).toHaveCount(0);
    await expect(page.locator(".agent-surface")).toHaveCount(0);
    await expect(page.locator(".agent-table")).toHaveCount(0);
  });

  test("footer renders on agents page", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
  });
});
