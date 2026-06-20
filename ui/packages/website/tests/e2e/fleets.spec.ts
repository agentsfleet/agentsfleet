import { test, expect } from "@playwright/test";

test.describe("Fleets page (/fleets)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/fleets");
  });

  test("renders Fleet-first heading", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous Fleets.",
    );
  });

  test("renders the merged install heading and npm command", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /install agentsfleet/i })).toBeVisible();
    await expect(page.getByLabel(/bootstrap commands/i)).toContainText(
      "npm install -g @agentsfleet/cli",
    );
  });

  test("renders install action links and no dashboard link", async ({ page }) => {
    await expect(page.getByRole("link", { name: /start a Fleet/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /read the docs/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /open dashboard/i })).toHaveCount(0);
  });

  test("renders bootstrap commands", async ({ page }) => {
    const block = page.getByLabel(/bootstrap commands/i);
    await expect(block).toBeVisible();
    await expect(block).toContainText("npm install -g @agentsfleet/cli");
    await expect(block).toContainText("agentsfleet login");
    await expect(block).toContainText("npx skills add agentsfleet/skills");
    await expect(block).toContainText("/agentsfleet-install-platform-ops");
  });

  test("renders machine surface heading + openapi link", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /machine surface/i })).toBeVisible();
    await expect(page.getByTestId("fleets-openapi-link")).toHaveAttribute("href", "/openapi.json");
  });

  test("renders API operations table", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /api operations/i })).toBeVisible();
    await expect(page.getByText("Create Fleet")).toBeVisible();
    await expect(page.getByText("Stop Fleet")).toBeVisible();
    await expect(page.getByText("Resume Fleet")).toBeVisible();
    await expect(page.getByText("Kill Fleet")).toBeVisible();
    await expect(page.getByText("Delete Fleet")).toBeVisible();
    await expect(page.getByText("Steer / chat")).toBeVisible();
  });

  test("renders webhook example", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /webhook ingest example/i })).toBeVisible();
    await expect(page.getByText(/deploy\.failed/)).toBeVisible();
  });

  test("renders safety limits as a constraint table", async ({ page }) => {
    await expect(page.getByRole("rowheader", { name: /^idempotency$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^audit trail$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^secret management$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^policy enforcement$/i })).toBeVisible();
  });

  test("does not render orange-era decorative chrome", async ({ page }) => {
    await expect(page.locator(".scanline")).toHaveCount(0);
    await expect(page.locator(".fleet-surface")).toHaveCount(0);
    await expect(page.locator(".fleet-table")).toHaveCount(0);
  });

  test("footer renders on fleets page", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
  });
});
